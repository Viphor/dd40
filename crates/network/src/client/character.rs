//! Client-side character replication systems.
//!
//! [`ClientCharacterPlugin`] is added automatically by [`NetworkCharacterPlugin`]
//! when the `client` feature is active.

use bevy::prelude::*;
use dd40_core::character::{Player, controller::CharacterInput, physics::PhysicsSet};
use lightyear::prelude::{
    Interpolated, Predicted,
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
};

use crate::{
    protocol::{NetworkCharacter, PlayerInput, PlayerPosition},
    shared::character::apply_input_to_controller,
};

// ============================================================================
// OBSERVERS
// ============================================================================

/// Attaches [`InputMarker<PlayerInput>`] and the [`Player`] marker to a
/// newly-created [`Predicted`] entity that belongs to a network character.
///
/// lightyear creates the `Predicted` entity and copies all registered
/// components (including [`NetworkCharacter`]) onto it before adding the
/// `Predicted` marker.  By the time this observer fires, the entity already
/// has `NetworkCharacter`, so the query is safe.
///
/// - [`InputMarker`] tells lightyear's input pipeline that this entity is the
///   one whose [`ActionState<PlayerInput>`] should be populated from the local
///   player's buffered inputs.
/// - [`Player`] makes `dd40_player`'s input systems (e.g. `player_input`,
///   `sync_camera_to_player`) work on the predicted entity without any
///   modification to the player crate.
fn on_predicted_character_added(
    trigger: On<Add, Predicted>,
    mut commands: Commands,
    query: Query<(), With<NetworkCharacter>>,
) {
    if query.get(trigger.entity).is_ok() {
        commands
            .entity(trigger.entity)
            .insert((InputMarker::<PlayerInput>::default(), Player));
        info!(
            "Attached InputMarker + Player to predicted character {:?}",
            trigger.entity
        );
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Reads [`CharacterInput`] (written by `dd40_player`'s input systems) and
/// forwards it into [`ActionState<PlayerInput>`] so lightyear can send it to
/// the server for the correct tick.
///
/// Runs in [`FixedPreUpdate`] inside [`InputSystems::WriteClientInputs`] so
/// lightyear picks the input up before it advances the tick counter.
///
/// This is a pure bridge — the network crate never reads the keyboard.
/// Movement intent is always sourced from [`CharacterInput`], which is written
/// by whatever input system owns the character (typically `dd40_player`'s
/// `player_input` system, or an AI system).
fn bridge_input_to_action_state(
    mut query: Query<
        (&CharacterInput, &mut ActionState<PlayerInput>),
        (
            With<NetworkCharacter>,
            With<Predicted>,
            With<InputMarker<PlayerInput>>,
        ),
    >,
) {
    for (char_input, mut action) in &mut query {
        action.0 = PlayerInput {
            movement: char_input.movement,
            jump: char_input.jump,
            sprint: char_input.sprint,
            pitch: char_input.pitch,
            yaw: char_input.yaw,
            place_block: false,
            remove_block: false,
        };
    }
}

/// Syncs the replicated [`PlayerPosition`] into [`Transform::translation`]
/// **before** physics runs on the [`Predicted`] entity.
///
/// lightyear's rollback mechanism restores `PlayerPosition` to a confirmed
/// checkpoint and then re-runs `FixedUpdate`.  Without this sync, physics
/// would re-simulate from the stale `Transform` instead of the rolled-back
/// position, causing divergence.
fn sync_position_to_transform(
    mut query: Query<(&PlayerPosition, &mut Transform), (With<NetworkCharacter>, With<Predicted>)>,
) {
    for (pos, mut transform) in &mut query {
        transform.translation = pos.to_vec3();
    }
}

/// Syncs the physics-resolved [`Transform::translation`] back to
/// [`PlayerPosition`] **after** physics finalises.
///
/// This keeps the prediction checkpoint (stored by lightyear in
/// `PlayerPosition`) up to date so rollbacks start from the correct position.
fn sync_transform_to_position(
    mut query: Query<(&Transform, &mut PlayerPosition), (With<NetworkCharacter>, With<Predicted>)>,
) {
    for (transform, mut pos) in &mut query {
        *pos = PlayerPosition::from_vec3(transform.translation);
    }
}

/// Applies the locally-controlled player's buffered [`PlayerInput`] to the
/// [`Predicted`] entity's [`CharacterInput`].
///
/// Uses the same [`apply_input_to_controller`] function as the server so
/// rollback prediction is deterministic.
fn client_apply_inputs(
    mut query: Query<
        (&ActionState<PlayerInput>, &mut CharacterInput),
        (With<NetworkCharacter>, With<Predicted>),
    >,
) {
    for (action, mut char_input) in &mut query {
        apply_input_to_controller(action, &mut char_input);
    }
}

/// Syncs the interpolated [`PlayerPosition`] to [`Transform`] for remote
/// player entities so they render at the smoothed position each frame.
///
/// This runs every frame in [`Update`] rather than `FixedUpdate` so rendering
/// is not locked to the fixed timestep.
fn sync_interpolated_position_to_transform(
    mut query: Query<
        (&PlayerPosition, &mut Transform),
        (With<NetworkCharacter>, With<Interpolated>),
    >,
) {
    for (pos, mut transform) in &mut query {
        transform.translation = pos.to_vec3();
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
        app.add_observer(on_predicted_character_added);

        app.add_systems(
            FixedPreUpdate,
            bridge_input_to_action_state.in_set(InputSystems::WriteClientInputs),
        );

        app.add_systems(
            FixedUpdate,
            (
                // 1. Restore rolled-back PlayerPosition into Transform before physics.
                sync_position_to_transform,
                // 2. Write input intent into CharacterController before physics integrate.
                client_apply_inputs,
            )
                .before(PhysicsSet::Integrate),
        );

        app.add_systems(
            FixedUpdate,
            // 3. Write physics-resolved position back to PlayerPosition after finalise.
            sync_transform_to_position.after(PhysicsSet::Finalise),
        );

        app.add_systems(
            Update,
            // Runs every render frame so interpolated remote players are smooth.
            sync_interpolated_position_to_transform,
        );
    }
}
