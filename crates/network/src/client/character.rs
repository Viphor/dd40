//! Client-side character replication systems.
//!
//! [`ClientCharacterPlugin`] is added automatically by [`NetworkCharacterPlugin`]
//! when the `client` feature is active.
//!
//! # Architecture
//!
//! The client uses three separate representations for a predicted character:
//!
//! | Component | Role |
//! |-----------|------|
//! | [`PlayerPosition`] | Network / rollback truth — lightyear checkpoints and restores this |
//! | [`CharacterPosition`] | Physics truth — the pipeline reads and writes this each tick |
//! | [`Transform`] | Visual truth — set by [`apply_frame_interpolation`] in `Update` |
//!
//! Each `FixedUpdate` tick:
//! 1. [`restore_and_record_previous`] detects rollbacks by comparing `PlayerPosition`
//!    (restored to the confirmed checkpoint by lightyear) against `CharacterPosition`
//!    (the last predicted result).  If they differ, a [`VisualCorrectionOffset`] is
//!    inserted.  Then `CharacterPosition` is synced from `PlayerPosition`.
//! 2. [`client_apply_inputs`] forwards buffered [`PlayerInput`] into [`CharacterInput`].
//! 3. The physics pipeline runs.
//! 4. [`record_and_sync_post_physics`] saves `CharacterPosition` as `current`, then
//!    copies it back to `PlayerPosition` so lightyear records the new prediction.
//!
//! Each render frame (`Update`):
//! - [`apply_frame_interpolation`] blends `previous` → `current` using the
//!   fixed-timestep overstep fraction and adds and decays any active
//!   [`VisualCorrectionOffset`].

use bevy::{prelude::*, time::Fixed};
use dd40_character_core::{
    builder::CharacterBuilder, controller::CharacterInput, system_sets::CharacterRenderSet,
};
use dd40_physics_core::character_ext::CharacterPhysicsExt;
use dd40_physics_core::prelude::{CharacterPosition, PhysicsSet};
use lightyear::prelude::{
    Interpolated, Predicted,
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
    is_in_rollback,
};

use crate::character_ext::CharacterClientNetworkExt;
use crate::{
    protocol::{NetworkCharacter, PlayerInput, PlayerPosition, PlayerRotation},
    shared::character::apply_input_to_controller,
};

// ============================================================================
// COMPONENTS
// ============================================================================

/// Stores the [`CharacterPosition`] from the two most recent physics ticks for
/// sub-tick frame interpolation.
///
/// Updated each `FixedUpdate` by [`restore_and_record_previous`] (writes
/// `previous`) and [`record_and_sync_post_physics`] (writes `current`).
/// Read each render frame by [`apply_frame_interpolation`].
#[derive(Component)]
pub struct PhysicsInterpolationData {
    /// Physics position at the **start** of the most recent tick (end of the
    /// previous tick), used as the interpolation origin.
    pub(crate) previous: Vec3,
    /// Physics position at the **end** of the most recent tick, used as the
    /// interpolation target.
    pub(crate) current: Vec3,
}

impl PhysicsInterpolationData {
    /// Creates a new [`PhysicsInterpolationData`] with both `previous` and
    /// `current` seeded to `pos`, suitable for the first tick after spawn.
    pub fn new(pos: Vec3) -> Self {
        Self {
            previous: pos,
            current: pos,
        }
    }
}

/// A decaying world-space offset added to [`Transform`] each render frame to
/// smooth out the visual "pop" caused by a prediction rollback.
///
/// Inserted by [`compute_visual_correction`] when a rollback moves the entity
/// by more than [`VISUAL_CORRECTION_MIN_ERROR`].  Removed automatically by
/// [`apply_frame_interpolation`] once the magnitude falls below
/// [`VISUAL_CORRECTION_THRESHOLD`].
#[derive(Component)]
struct VisualCorrectionOffset(Vec3);

/// Minimum rollback displacement (in world units) that triggers a
/// [`VisualCorrectionOffset`].  Smaller corrections are applied instantly.
const VISUAL_CORRECTION_MIN_ERROR: f32 = 0.05;

/// Once the [`VisualCorrectionOffset`] magnitude drops below this threshold
/// (in world units) the component is removed and the correction stops.
const VISUAL_CORRECTION_THRESHOLD: f32 = 0.001;

/// How quickly the [`VisualCorrectionOffset`] decays toward zero, expressed as
/// a lerp blend rate per second.  Higher values produce a snappier but more
/// noticeable correction; lower values are smoother but take longer to settle.
const VISUAL_CORRECTION_DECAY: f32 = 15.0;

// ============================================================================
// OBSERVERS
// ============================================================================

/// Attaches local-player components to a newly-replicated character body.
///
/// Triggered on `Add<NetworkCharacter>`, which fires once per replicated
/// character body — never for the face child or other replicated children
/// (which don't carry the `NetworkCharacter` marker). This guarantees we
/// don't accidentally treat a child entity as a character body and double
/// up `InputMarker` insertions.
///
/// The observer fires for both copies that arrive on the client:
/// * The **Confirmed** entity (server-replicated truth) — skipped here; it
///   exists only as a rollback checkpoint.
/// * The **Predicted** entity (local player's controllable copy) — gets the
///   full character bundle plus local-player extras (`InputMarker`,
///   `Player`, `PhysicsInterpolationData`).
///
/// Once remote-character rendering lands, an `Has<Interpolated>` branch can
/// be added here to set up read-only render state for other players.
fn on_network_character_added(
    trigger: On<Add, NetworkCharacter>,
    mut commands: Commands,
    query: Query<(&PlayerPosition, Has<Predicted>)>,
) {
    let Ok((player_pos, is_predicted)) = query.get(trigger.entity) else {
        return;
    };

    if !is_predicted {
        return;
    }

    let initial_pos = player_pos.to_vec3();
    let mut entity_cmds = commands.entity(trigger.entity);
    CharacterBuilder::new("ThePlayer")
        .transform(Transform::from_translation(initial_pos))
        .with_physics()
        .with_controller()
        .with_predicted_local_player(initial_pos)
        .attach(&mut entity_cmds);

    info!(
        "Built local-player character on Predicted entity {:?}",
        trigger.entity
    );
}

// ============================================================================
// FIXED-UPDATE SYSTEMS  (run inside lightyear's rollback replay loop)
// ============================================================================

/// Reads [`CharacterInput`] and forwards it into [`ActionState<PlayerInput>`]
/// so lightyear buffers it for the current tick and sends it to the server.
///
/// Runs in [`FixedPreUpdate`] inside [`InputSystems::WriteClientInputs`] so
/// lightyear sees the fresh input **before** it advances the tick counter.
/// Placing it here (instead of `FixedUpdate`) eliminates a 1-tick delay on
/// one-shot flags like [`CharacterInput::jump`].
///
/// [`CharacterInput::jump`]: dd40_core::character::controller::CharacterInput::jump
fn bridge_input_to_action_state(
    mut query: Query<
        (&CharacterInput, &mut ActionState<PlayerInput>),
        (With<Predicted>, With<InputMarker<PlayerInput>>),
    >,
) {
    for (char_input, mut action) in &mut query {
        action.0 = PlayerInput {
            movement: char_input.movement,
            jump: char_input.jump,
            sprint: char_input.sprint,
            pitch: char_input.pitch,
            yaw: char_input.yaw,
            attack: char_input.attack,
            interact: char_input.interact,
            place: char_input.place,
        };
    }
}

/// Saves the current [`CharacterPosition`] as the interpolation `previous`
/// value, then restores it from the lightyear rollback checkpoint
/// ([`PlayerPosition`]).
///
/// Also detects rollbacks: when `PlayerPosition` (restored by lightyear to a
/// confirmed checkpoint) differs from `CharacterPosition` (the last predicted
/// result), a [`VisualCorrectionOffset`] is inserted so the rendered entity
/// slides smoothly to the corrected position rather than popping.
///
/// Must run **before** [`PhysicsSet::Integrate`] so physics always starts from
/// the rolled-back position, not stale visual state.
fn restore_and_record_previous(
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &PlayerPosition,
            &mut CharacterPosition,
            &mut PhysicsInterpolationData,
            Option<&VisualCorrectionOffset>,
        ),
        With<Predicted>,
    >,
) {
    for (entity, player_pos, mut char_pos, mut interp, existing_correction) in &mut query {
        let new_physics_pos = player_pos.to_vec3();

        // Detect rollback: lightyear just restored PlayerPosition to a
        // confirmed checkpoint that differs from our last predicted position.
        // The visual error is how far the entity appears to jump — we decay it
        // over the next few render frames instead of snapping.
        let delta = char_pos.0 - new_physics_pos;
        if delta.length() > VISUAL_CORRECTION_MIN_ERROR {
            let total = delta + existing_correction.map_or(Vec3::ZERO, |e| e.0);
            if total.length() > VISUAL_CORRECTION_MIN_ERROR {
                commands.entity(entity).insert(VisualCorrectionOffset(total));
            }
        }

        // Record where physics was at the end of the last tick — this becomes
        // the interpolation origin for the current render frame window.
        interp.previous = char_pos.0;
        // Restore the physics position from the lightyear checkpoint so
        // rollback re-simulation starts from the correct confirmed state.
        char_pos.0 = new_physics_pos;
    }
}

/// Applies the locally-buffered [`PlayerInput`] to the predicted entity's
/// [`CharacterInput`], using the same logic as the server for determinism.
///
/// Must run **before** [`PhysicsSet::Integrate`] so the physics step uses the
/// current tick's input.
fn client_apply_inputs(
    mut query: Query<(&ActionState<PlayerInput>, &mut CharacterInput), With<Predicted>>,
) {
    for (action, mut char_input) in &mut query {
        apply_input_to_controller(action, &mut char_input);
    }
}

/// Records the post-physics [`CharacterPosition`] as the interpolation
/// `current` value, then copies it back to [`PlayerPosition`] so lightyear
/// stores this prediction in its history for future rollback comparisons.
///
/// Must run **after** [`PhysicsSet::Finalise`].
fn record_and_sync_post_physics(
    mut query: Query<
        (
            &CharacterPosition,
            &mut PlayerPosition,
            &mut PhysicsInterpolationData,
        ),
        With<Predicted>,
    >,
) {
    for (char_pos, mut player_pos, mut interp) in &mut query {
        interp.current = char_pos.0;
        *player_pos = PlayerPosition::from_vec3(char_pos.0);
    }
}

// ============================================================================
// UPDATE SYSTEMS  (run every render frame)
// ============================================================================

/// Blends the predicted entity's [`Transform`] between the two most recent
/// physics tick positions using the fixed-timestep overstep fraction, then
/// adds and decays any active [`VisualCorrectionOffset`].
///
/// This gives the local player smooth sub-tick motion at any frame rate while
/// keeping the physics simulation running at its fixed rate.  The correction
/// offset gradually fades after a rollback so the entity slides smoothly to
/// the physics-correct position rather than snapping.
fn apply_frame_interpolation(
    fixed_time: Res<Time<Fixed>>,
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &PhysicsInterpolationData,
            Option<&mut VisualCorrectionOffset>,
            &mut Transform,
        ),
        With<Predicted>,
    >,
) {
    let overstep = fixed_time.overstep_fraction();
    let dt = time.delta_secs();

    for (entity, interp, correction, mut transform) in &mut query {
        transform.translation = interp.previous.lerp(interp.current, overstep);

        if let Some(mut offset) = correction {
            transform.translation += offset.0;

            offset.0 = offset
                .0
                .lerp(Vec3::ZERO, (VISUAL_CORRECTION_DECAY * dt).min(1.0));

            if offset.0.length_squared() < VISUAL_CORRECTION_THRESHOLD * VISUAL_CORRECTION_THRESHOLD
            {
                commands.entity(entity).remove::<VisualCorrectionOffset>();
            }
        }
    }
}

/// Syncs the snapshot-interpolated [`PlayerPosition`] to [`Transform`] for
/// remote player entities so they render at the smoothed position each frame.
fn sync_interpolated_position_to_transform(
    mut query: Query<
        (&PlayerPosition, &mut Transform),
        With<Interpolated>,
    >,
) {
    for (pos, mut transform) in &mut query {
        transform.translation = pos.to_vec3();
    }
}

/// Applies the interpolated [`PlayerRotation`] to remote players' [`Transform`]
/// each render frame so other clients see smooth head rotation.
///
/// Runs only for [`Interpolated`] entities — the controlling client's predicted
/// entity has its camera rotation driven directly by `mouse_look`, not by
/// `PlayerRotation`.
fn apply_interpolated_rotation(
    mut query: Query<
        (&PlayerRotation, &mut Transform),
        With<Interpolated>,
    >,
) {
    for (rot, mut transform) in &mut query {
        transform.rotation = Quat::from_euler(EulerRot::YXZ, rot.yaw, rot.pitch, 0.0);
    }
}

/// Writes the local player's current camera orientation into [`PlayerRotation`]
/// so the server can replicate it to other clients.
///
/// Runs in `PostUpdate` — after `player_input` (Update) has already copied the
/// latest `CameraRotation` into `CharacterInput`, and after lightyear's
/// replication receive (PreUpdate) may have overwritten the component with a
/// stale server-confirmed value.  Running here guarantees the render pass
/// always sees the locally-driven, zero-lag rotation rather than a rolled-back
/// one.
fn sync_local_rotation(
    mut query: Query<
        (&CharacterInput, &mut PlayerRotation),
        With<Predicted>,
    >,
) {
    for (char_input, mut player_rot) in &mut query {
        player_rot.pitch = char_input.pitch;
        player_rot.yaw = char_input.yaw;
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

/// Client-side character replication plugin.
///
/// Registered automatically by [`NetworkCharacterPlugin`] when the `client`
/// feature is active.
pub struct ClientCharacterPlugin;

impl Plugin for ClientCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_network_character_added);

        // Bridge CharacterInput → ActionState in FixedPreUpdate so lightyear
        // buffers the current frame's input (including the one-shot jump flag)
        // before it advances the tick counter.  Skip during rollback: lightyear
        // already restores the historical ActionState for each replayed tick, and
        // running the bridge would overwrite it with stale CharacterInput values
        // (e.g. jump=false after apply_character_controller already consumed it).
        app.add_systems(
            FixedPreUpdate,
            bridge_input_to_action_state
                .in_set(InputSystems::WriteClientInputs)
                .run_if(not(is_in_rollback)),
        );

        app.add_systems(
            FixedUpdate,
            (restore_and_record_previous, client_apply_inputs).in_set(PhysicsSet::InputSync),
        );

        app.add_systems(
            FixedUpdate,
            record_and_sync_post_physics.after(PhysicsSet::Finalise),
        );

        app.add_systems(
            Update,
            (
                apply_frame_interpolation.in_set(CharacterRenderSet::FrameInterpolation),
                sync_interpolated_position_to_transform,
                apply_interpolated_rotation,
            ),
        );

        // Write the current camera orientation into PlayerRotation every frame,
        // after player_input (Update) has refreshed CharacterInput and after
        // lightyear's replication receive (PreUpdate) may have overwritten it.
        app.add_systems(PostUpdate, sync_local_rotation);
    }
}
