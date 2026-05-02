use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use dd40_character_core::components::Player;
use dd40_character_core::controller::CharacterInput;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::{BlockPos, ChunkPos, GameState};

use crate::components::{CameraRotation, MouseSensitivity};
use crate::state::PlayerMode;

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the first-person camera entity on entering [`AppState::Playing`].
pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 64.0, 0.0),
        CameraRotation::default(),
        MouseSensitivity::default(),
    ));
}

// ---------------------------------------------------------------------------
// Cursor management
// ---------------------------------------------------------------------------

pub(crate) fn on_pause(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut opts) = cursor_options.single_mut() {
        opts.visible = true;
        opts.grab_mode = CursorGrabMode::None;
    }
}

pub(crate) fn on_resume(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut opts) = cursor_options.single_mut() {
        opts.visible = false;
        opts.grab_mode = CursorGrabMode::Locked;
    }
}

pub(crate) fn pause_on_escape(
    game_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Running => next_state.set(GameState::Paused),
            GameState::Paused => next_state.set(GameState::Running),
        }
    }
}

// ---------------------------------------------------------------------------
// Mode management
// ---------------------------------------------------------------------------

/// Toggles between [`PlayerMode::Controller`] and [`PlayerMode::FreeCam`] on
/// **F1**.
pub(crate) fn toggle_player_mode(
    mode: Res<State<PlayerMode>>,
    mut next_mode: ResMut<NextState<PlayerMode>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::F1) {
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
}

// ---------------------------------------------------------------------------
// Controller mode — input → CharacterInput
// ---------------------------------------------------------------------------

/// Reads keyboard and camera state and writes movement intent into
/// [`CharacterInput`].
///
/// Runs only in [`PlayerMode::Controller`].
pub(crate) fn player_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Transform, &CameraRotation), With<Camera3d>>,
    mut player_query: Query<&mut CharacterInput, With<Player>>,
) {
    let Ok(mut char_input) = player_query.single_mut() else {
        return;
    };
    let Ok((camera_transform, camera_rotation)) = camera_query.single() else {
        return;
    };

    let forward = camera_transform.forward();
    let right = camera_transform.right();
    let forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_h = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += forward_h;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= forward_h;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= right_h;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += right_h;
    }

    char_input.movement = direction.normalize_or_zero();
    char_input.jump |= keyboard.just_pressed(KeyCode::Space);
    char_input.sprint = keyboard.pressed(KeyCode::ControlLeft);
    char_input.pitch = camera_rotation.pitch;
    char_input.yaw = camera_rotation.yaw;
}

// ---------------------------------------------------------------------------
// FreeCam mode
// ---------------------------------------------------------------------------

/// Moves the camera entity directly, bypassing physics.
///
/// Runs only in [`PlayerMode::FreeCam`].
pub(crate) fn free_cam_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(mut transform) = camera_query.single_mut() else {
        return;
    };

    let forward = transform.forward();
    let right = transform.right();
    let forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_h = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += forward_h;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= forward_h;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= right_h;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += right_h;
    }
    if keyboard.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) {
        direction -= Vec3::Y;
    }

    let sprint = if keyboard.pressed(KeyCode::ControlLeft) {
        2.0
    } else {
        1.0
    };

    const FREE_CAM_SPEED: f32 = 10.0;
    if direction != Vec3::ZERO {
        transform.translation +=
            direction.normalize() * FREE_CAM_SPEED * sprint * time.delta_secs();
    }
}

// ---------------------------------------------------------------------------
// Shared camera systems
// ---------------------------------------------------------------------------

/// Handles mouse movement to rotate the camera.
///
/// Runs in both [`PlayerMode::Controller`] and [`PlayerMode::FreeCam`].
pub(crate) fn mouse_look(
    accumulated_mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<
        (&mut Transform, &mut CameraRotation, &MouseSensitivity),
        With<Camera3d>,
    >,
    cursor_options: Query<&CursorOptions>,
) {
    let Ok(cursor_option) = cursor_options.single() else {
        return;
    };
    if cursor_option.grab_mode != CursorGrabMode::Locked {
        return;
    }

    let Ok((mut transform, mut rotation, sensitivity)) = camera_query.single_mut() else {
        return;
    };

    let ev = accumulated_mouse_motion;
    rotation.yaw -= ev.delta.x * sensitivity.0;
    rotation.pitch -= ev.delta.y * sensitivity.0;
    rotation.pitch = rotation.pitch.clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    transform.rotation = Quat::from_euler(EulerRot::YXZ, rotation.yaw, rotation.pitch, 0.0);
}

/// Syncs the camera translation to the player entity's position.
///
/// Runs only in [`PlayerMode::Controller`].
pub(crate) fn sync_camera_to_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };
    camera_transform.translation = player_transform.translation + Vec3::new(0.0, 1.6, 0.0);
}

// ---------------------------------------------------------------------------
// Debug info
// ---------------------------------------------------------------------------

/// Inserts a [`DebugInfo`] panel on newly-spawned player entities.
pub(crate) fn add_debug_info(
    mut commands: Commands,
    player_query: Query<Entity, Added<Player>>,
) {
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

/// Requests nearby chunks around the player each frame.
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
                z: player_chunk_pos.z + dz,
            };
            if !chunk_cache.contains(&chunk_pos) {
                chunk_cache.request(chunk_pos);
            }
        }
    }
}
