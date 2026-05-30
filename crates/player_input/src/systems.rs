//! Client-only systems and observers for the locally-controlled player.
//!
//! Reads BEI action state (set up by [`crate::bindings`]) and translates it
//! into the camera, cursor, mode, and chunk-loading behaviours that drive
//! the local play experience. The networked `OnFoot` action state is
//! translated into [`CharacterInput`] by
//! [`crate::translation::apply_actions_to_character_input`] — these
//! systems only own client-local concerns.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_enhanced_input::prelude::{Action, ActionOf, ContextActivity, Fire, TriggerState};
use dd40_character_core::components::Player;
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::face::{CameraRotation as FaceRotation, CharacterFace};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::{BlockId, BlockPos, ChunkPos, GameState, OpenUiWindows};
use dd40_input_core::actions::{
    FreeCamDown, FreeCamUp, Interact, Look, Move, Pause, Place, RmbPress, Sprint, ToggleFreeCam,
};
use dd40_input_core::contexts::OnFoot;
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::ItemRegistry;

use crate::contexts::{FreeCam, LocalUi};
use crate::state::PlayerMode;

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the first-person [`Camera3d`] entity on entering `AppState::Playing`.
pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 64.0, 0.0)));
}

// ---------------------------------------------------------------------------
// Cursor management
// ---------------------------------------------------------------------------

/// Reconciles the cursor grab + visibility from
/// [`GameState`] and [`OpenUiWindows`].
///
/// The cursor is released (visible, ungrabbed) whenever **either**:
/// - the game is paused, **or**
/// - at least one UI window registered in [`OpenUiWindows`] has
///   `releases_cursor = true`.
///
/// Otherwise the cursor is locked (hidden, grabbed) so first-person
/// look works.  Runs every frame but only writes to the
/// [`CursorOptions`] component when the desired state differs from
/// the current state, so it remains cheap.
pub(crate) fn reconcile_cursor_grab(
    game_state: Res<State<GameState>>,
    windows: Res<OpenUiWindows>,
    mut cursor_options: Query<&mut CursorOptions>,
) {
    let Ok(mut opts) = cursor_options.single_mut() else {
        return;
    };
    let should_release =
        matches!(game_state.get(), GameState::Paused) || windows.cursor_should_release();
    let desired_grab = if should_release {
        CursorGrabMode::None
    } else {
        CursorGrabMode::Locked
    };
    let desired_visible = should_release;
    if opts.grab_mode != desired_grab {
        opts.grab_mode = desired_grab;
    }
    if opts.visible != desired_visible {
        opts.visible = desired_visible;
    }
}

// ---------------------------------------------------------------------------
// Pause + mode-toggle observers (driven by LocalUi actions)
// ---------------------------------------------------------------------------

/// Toggles [`GameState`] between `Running` and `Paused` whenever the
/// [`Pause`] action fires. Runs only while in `AppState::Playing`; the
/// caller is responsible for not feeding the action outside that state
/// (the BEI binding for `Pause` is only attached to the local player, who
/// only exists in `Playing`).
pub(crate) fn on_pause_action(
    _: On<Fire<Pause>>,
    game_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    match game_state.get() {
        GameState::Running => next_state.set(GameState::Paused),
        GameState::Paused => next_state.set(GameState::Running),
    }
}

/// Toggles [`PlayerMode`] between `Controller` and `FreeCam` on every
/// [`ToggleFreeCam`] fire.
pub(crate) fn on_toggle_free_cam_action(
    _: On<Fire<ToggleFreeCam>>,
    mode: Res<State<PlayerMode>>,
    mut next_mode: ResMut<NextState<PlayerMode>>,
) {
    match mode.get() {
        PlayerMode::Controller => {
            info!("Switching to FreeCam mode");
            next_mode.set(PlayerMode::FreeCam);
        }
        PlayerMode::FreeCam => {
            info!("Switching to Controller mode");
            next_mode.set(PlayerMode::Controller);
        }
    }
}

/// Swaps the OnFoot / FreeCam context activity on player-mode transitions.
///
/// Both contexts always live on the local player entity; only one is
/// `ContextActivity::ACTIVE` at a time so input flows to the correct set
/// of consumers.
pub(crate) fn sync_context_activity_to_mode(
    mode: Res<State<PlayerMode>>,
    players: Query<Entity, With<Player>>,
    mut commands: Commands,
) {
    let Ok(player) = players.single() else {
        return;
    };
    let (on_foot, free_cam) = match mode.get() {
        PlayerMode::Controller => (
            ContextActivity::<OnFoot>::ACTIVE,
            ContextActivity::<FreeCam>::INACTIVE,
        ),
        PlayerMode::FreeCam => (
            ContextActivity::<OnFoot>::INACTIVE,
            ContextActivity::<FreeCam>::ACTIVE,
        ),
    };
    commands.entity(player).insert((on_foot, free_cam));
}

// ---------------------------------------------------------------------------
// RMB dispatch — Place vs Interact based on active item
// ---------------------------------------------------------------------------

/// Observer for [`RmbPress`]: chooses between [`Place`] and [`Interact`]
/// based on the player's [`ActiveItem`] and mocks one tick of the chosen
/// networked action so lightyear buffers + replicates it to the server.
///
/// Using [`bevy_enhanced_input::prelude::ActionMock`] (rather than mutating
/// the action value directly) keeps the action's `Fire` event firing
/// correctly for downstream observers in both client and server.
pub(crate) fn on_rmb_press(
    trigger: On<Fire<RmbPress>>,
    items: Res<ItemRegistry>,
    actives: Query<Option<&ActiveItem>, With<Player>>,
    place_actions: Query<(Entity, &ActionOf<OnFoot>), With<Action<Place>>>,
    interact_actions: Query<(Entity, &ActionOf<OnFoot>), With<Action<Interact>>>,
    mut commands: Commands,
) {
    let player = trigger.context;
    let Ok(active) = actives.get(player) else {
        return;
    };

    if has_placeable(active, &items) {
        if let Some((action, _)) = place_actions.iter().find(|(_, of)| ***of == player) {
            commands
                .entity(action)
                .insert(bevy_enhanced_input::prelude::ActionMock::once(
                    TriggerState::Fired,
                    true,
                ));
        }
    } else if let Some((action, _)) = interact_actions.iter().find(|(_, of)| ***of == player) {
        commands
            .entity(action)
            .insert(bevy_enhanced_input::prelude::ActionMock::once(
                TriggerState::Fired,
                true,
            ));
    }
}

fn has_placeable(active: Option<&ActiveItem>, items: &ItemRegistry) -> bool {
    let Some(stack) = active.and_then(|a| a.peek()) else {
        return false;
    };
    let Some(def) = items.get(stack.item) else {
        return false;
    };
    matches!(def.placeable, Some(b) if b != BlockId::AIR)
}

// ---------------------------------------------------------------------------
// Mouse look — integrates Look delta into the face's persistent rotation
// ---------------------------------------------------------------------------

/// Integrates the [`Look`] action delta into the local player's
/// [`FaceRotation`] (clamping pitch), then writes the resulting quaternion
/// onto the face's local [`Transform`] and copies pitch/yaw into the
/// player's [`CharacterInput`] so the network layer ships them inside
/// [`PlayerInput`](dd40_network) each tick.
///
/// Sensitivity is applied at the binding layer (see
/// [`crate::bindings::DEFAULT_MOUSE_SENSITIVITY`]); this system consumes
/// the post-modifier value directly.
pub(crate) fn mouse_look(
    look_actions: Query<(&Action<Look>, &ActionOf<LocalUi>)>,
    mut face_query: Query<(&mut Transform, &mut FaceRotation, &ChildOf), With<CharacterFace>>,
    mut player_query: Query<&mut CharacterInput, With<Player>>,
    cursor_options: Query<&CursorOptions>,
) {
    let Ok(cursor_option) = cursor_options.single() else {
        return;
    };
    if cursor_option.grab_mode != CursorGrabMode::Locked {
        return;
    }

    for (mut transform, mut rotation, child_of) in &mut face_query {
        let parent = child_of.parent();
        let Ok(mut char_input) = player_query.get_mut(parent) else {
            continue;
        };

        let delta = look_actions
            .iter()
            .find_map(|(action, of)| (**of == parent).then(|| **action))
            .unwrap_or(Vec2::ZERO);

        // Action<Look> already has DEFAULT_MOUSE_SENSITIVITY applied by the
        // Scale modifier and is negated, so this is the raw integrated
        // delta to add.
        rotation.yaw += delta.x;
        rotation.pitch += delta.y;
        rotation.pitch = rotation.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );

        transform.rotation = Quat::from_euler(EulerRot::YXZ, rotation.yaw, rotation.pitch, 0.0);

        // Mirror onto CharacterInput so the camera-rotation bridge picks
        // up the persistent orientation.
        char_input.yaw = rotation.yaw;
        char_input.pitch = rotation.pitch;
    }
}

// ---------------------------------------------------------------------------
// FreeCam mode
// ---------------------------------------------------------------------------

const FREE_CAM_SPEED: f32 = 10.0;

/// Rotates the camera entity directly from the [`Look`] action while in
/// [`PlayerMode::FreeCam`]. The player's [`CharacterFace`] is left
/// untouched — the head stays where the player was looking when freecam
/// was entered, and [`CharacterInput::yaw`] / [`pitch`] are likewise not
/// modified (so the server keeps the controller's last-known orientation).
pub(crate) fn free_cam_look(
    look_actions: Query<(&Action<Look>, &ActionOf<LocalUi>)>,
    players: Query<Entity, With<Player>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
    mut state: Local<FreeCamLookState>,
    cursor_options: Query<&CursorOptions>,
) {
    let Ok(cursor_option) = cursor_options.single() else {
        return;
    };
    if cursor_option.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(player) = players.single() else {
        return;
    };
    let Ok(mut transform) = camera_query.single_mut() else {
        return;
    };

    // Re-seed pitch/yaw from the camera transform if it changed externally
    // (e.g. just transitioned into FreeCam and sync_camera_to_face placed
    // it at the face orientation). Detect by comparing against the
    // transform produced by our last write.
    if state.last_applied.is_none()
        || state
            .last_applied
            .map(|q| q.angle_between(transform.rotation) > 1e-4)
            .unwrap_or(true)
    {
        let (y, p, _) = transform.rotation.to_euler(EulerRot::YXZ);
        state.yaw = y;
        state.pitch = p;
    }

    let delta = look_actions
        .iter()
        .find_map(|(action, of)| (**of == player).then(|| **action))
        .unwrap_or(Vec2::ZERO);

    state.yaw += delta.x;
    state.pitch = (state.pitch + delta.y).clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    let rot = Quat::from_euler(EulerRot::YXZ, state.yaw, state.pitch, 0.0);
    transform.rotation = rot;
    state.last_applied = Some(rot);
}

/// Per-system state for [`free_cam_look`].
#[derive(Default)]
pub(crate) struct FreeCamLookState {
    yaw: f32,
    pitch: f32,
    last_applied: Option<Quat>,
}

/// Moves the camera entity directly, bypassing physics. Reads
/// `Action<Move>` / `Action<FreeCamUp>` / `Action<FreeCamDown>` /
/// `Action<Sprint>` from the [`FreeCam`] context.
pub(crate) fn free_cam_movement(
    time: Res<Time>,
    players: Query<Entity, With<Player>>,
    moves: Query<(&Action<Move>, &ActionOf<FreeCam>)>,
    ups: Query<(&Action<FreeCamUp>, &ActionOf<FreeCam>)>,
    downs: Query<(&Action<FreeCamDown>, &ActionOf<FreeCam>)>,
    sprints: Query<(&Action<Sprint>, &ActionOf<FreeCam>)>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(player) = players.single() else {
        return;
    };
    let Ok(mut transform) = camera_query.single_mut() else {
        return;
    };

    let planar = moves
        .iter()
        .find_map(|(a, of)| (**of == player).then(|| **a))
        .unwrap_or(Vec2::ZERO);
    let up = ups
        .iter()
        .find_map(|(a, of)| (**of == player).then(|| **a))
        .unwrap_or(false);
    let down = downs
        .iter()
        .find_map(|(a, of)| (**of == player).then(|| **a))
        .unwrap_or(false);
    let sprint = sprints
        .iter()
        .find_map(|(a, of)| (**of == player).then(|| **a))
        .unwrap_or(false);

    let forward = transform.forward();
    let right = transform.right();
    let forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_h = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut direction = right_h * planar.x + forward_h * planar.y;
    if up {
        direction += Vec3::Y;
    }
    if down {
        direction -= Vec3::Y;
    }

    let sprint = if sprint { 2.0 } else { 1.0 };
    if direction != Vec3::ZERO {
        transform.translation +=
            direction.normalize() * FREE_CAM_SPEED * sprint * time.delta_secs();
    }
}

// ---------------------------------------------------------------------------
// Shared camera sync
// ---------------------------------------------------------------------------

pub(crate) fn sync_camera_to_face(
    face_query: Query<(&GlobalTransform, &ChildOf), With<CharacterFace>>,
    player_query: Query<(), With<Player>>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Some(face_global) = face_query
        .iter()
        .find_map(|(gt, child_of)| player_query.get(child_of.parent()).ok().map(|_| gt))
    else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };
    let (_scale, rotation, translation) = face_global.to_scale_rotation_translation();
    camera_transform.translation = translation;
    camera_transform.rotation = rotation;
}

// ---------------------------------------------------------------------------
// Debug info
// ---------------------------------------------------------------------------

pub(crate) fn add_debug_info(mut commands: Commands, player_query: Query<Entity, Added<Player>>) {
    use bevy::color::palettes::basic::YELLOW;
    for player_entity in player_query.iter() {
        commands.entity(player_entity).insert(
            DebugInfo::new("Player Info")
                .with_color(YELLOW.into())
                .add("position", "Player position")
                .add("velocity", "Player velocity")
                .add("impulse", "Player impulse")
                .add("chunk", "Player chunk"),
        );
    }
}

// ---------------------------------------------------------------------------
// Chunk loading
// ---------------------------------------------------------------------------

pub(crate) fn load_nearby_chunks(
    mut chunk_cache: ResMut<ChunkCache>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = BlockPos::from(player_transform);
    let player_chunk_pos = player_pos.chunk_pos();

    for dz in -1..=1 {
        for dx in -1..=1 {
            let chunk_pos = ChunkPos {
                x: player_chunk_pos.x + dx,
                y: player_chunk_pos.y,
                z: player_chunk_pos.z + dz,
            };
            if !chunk_cache.contains(&chunk_pos) {
                chunk_cache.request(chunk_pos);
            }
        }
    }
}
