use bevy::color::palettes::basic::YELLOW;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use dd40_core::character::{CharacterBuilder, MovementSpeed, Player};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;

pub mod block_interaction;

pub use block_interaction::{BlockInteractionConfig, BlockInteractionPlugin, TargetedBlock};

/// Mouse sensitivity for looking around.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self(0.002)
    }
}

/// Pitch and yaw angles for the camera.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct CameraRotation {
    pub pitch: f32,
    pub yaw: f32,
}

impl Default for CameraRotation {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Player,
        CharacterBuilder::new("Player")
            .transform(Transform::from_xyz(0.0, 64.0, 0.0))
            .build(),
        DebugInfo::new("Player Info")
            .with_color(YELLOW.into())
            .add("position", "Player position")
            .add("chunk", "Player chunk"),
    ));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 64.0, 0.0),
        CameraRotation::default(),
        MouseSensitivity::default(),
    ));
}

fn update_debug_info(mut player_query: Single<(&Transform, &mut DebugInfo), With<Player>>) {
    let pos = player_query.0.translation;
    player_query.1.set(
        "position",
        format!("({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z),
    );
    let chunk = BlockPos::from(player_query.0).chunk_pos();
    player_query.1.set("chunk", chunk.to_string());
}

fn on_pause(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut cursor_option) = cursor_options.single_mut() {
        cursor_option.visible = true;
        cursor_option.grab_mode = CursorGrabMode::None;
    }
}

fn on_resume(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut cursor_option) = cursor_options.single_mut() {
        cursor_option.visible = false;
        cursor_option.grab_mode = CursorGrabMode::Locked;
    }
}

// fn grab_cursor(
//     mut cursor_options: Query<&mut CursorOptions>,
//     mouse: Res<ButtonInput<MouseButton>>,
//     key: Res<ButtonInput<KeyCode>>,
// ) {
//     let Ok(mut cursor_option) = cursor_options.single_mut() else {
//         return;
//     };
//
//     if mouse.just_pressed(MouseButton::Left) {
//         cursor_option.visible = false;
//         cursor_option.grab_mode = CursorGrabMode::Locked;
//     }
//
//     if key.just_pressed(KeyCode::Escape) {
//         cursor_option.visible = true;
//         cursor_option.grab_mode = CursorGrabMode::None;
//     }
// }

fn pause_on_escape(
    game_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Running => {
                next_state.set(GameState::Paused);
            }
            GameState::Paused => {
                next_state.set(GameState::Running);
            }
        }
    }
}

/// Moves the player entity using WASD / Space / Left-Shift.
fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut player_query: Query<(&mut Transform, &MovementSpeed), With<Player>>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<Player>)>,
) {
    let Ok((mut transform, speed)) = player_query.single_mut() else {
        return;
    };

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let mut direction = Vec3::ZERO;

    // Get the forward and right vectors from the camera's rotation
    let forward = camera_transform.forward();
    let right = camera_transform.right();

    // Project onto horizontal plane (ignore Y component for movement)
    let forward_horizontal = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_horizontal = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    if keyboard.pressed(KeyCode::KeyW) {
        direction += forward_horizontal;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= forward_horizontal;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= right_horizontal;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += right_horizontal;
    }
    if keyboard.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) {
        direction -= Vec3::Y;
    }

    if direction != Vec3::ZERO {
        transform.translation += direction.normalize() * speed.0 * time.delta_secs();
    }
}

/// Handles mouse movement to rotate the camera.
fn mouse_look(
    accumulated_mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<
        (&mut Transform, &mut CameraRotation, &MouseSensitivity),
        With<Camera3d>,
    >,
    cursor_options: Query<&CursorOptions>,
) {
    //let window = windows.single();
    let Ok(cursor_option) = cursor_options.single() else {
        return;
    };

    // Only process mouse movement if cursor is grabbed
    if cursor_option.grab_mode != CursorGrabMode::Locked {
        return;
    }

    let Ok((mut transform, mut rotation, sensitivity)) = camera_query.single_mut() else {
        return;
    };

    let ev = accumulated_mouse_motion;
    rotation.yaw -= ev.delta.x * sensitivity.0;
    rotation.pitch -= ev.delta.y * sensitivity.0;

    // Clamp pitch to prevent camera flipping
    rotation.pitch = rotation.pitch.clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    // Apply rotation to transform
    transform.rotation = Quat::from_euler(EulerRot::YXZ, rotation.yaw, rotation.pitch, 0.0);
}

/// Syncs the camera position with the player position.
fn sync_camera_to_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    camera_transform.translation = player_transform.translation;
}

fn load_nearby_chunks(
    mut chunk_cache: ResMut<ChunkCache>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = BlockPos::from(player_transform);
    let player_chunk_pos = player_pos.chunk_pos();

    // Load chunks in a 3x3 area around the player
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

/// Bevy plugin that registers player types and spawns the player on startup.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockInteractionPlugin::default())
            .register_type::<MovementSpeed>()
            .register_type::<MouseSensitivity>()
            .register_type::<CameraRotation>()
            .add_systems(Startup, (spawn_player, setup_camera))
            .add_systems(OnEnter(GameState::Paused), on_pause)
            .add_systems(OnEnter(GameState::Running), on_resume)
            .add_systems(
                PreUpdate,
                load_nearby_chunks
                    .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
            )
            .add_systems(
                Update,
                (
                    //grab_cursor,
                    mouse_look,
                    player_movement,
                    sync_camera_to_player,
                    update_debug_info,
                )
                    .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
            )
            .add_systems(Update, pause_on_escape.run_if(in_state(AppState::Playing)));
    }
}
