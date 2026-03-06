use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;

/// Marker component that identifies the player entity.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Player;

/// Walking / flying speed of the player in units per second.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

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
        MovementSpeed::default(),
        Transform::from_xyz(0.0, 64.0, 0.0),
        GlobalTransform::default(),
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

fn grab_cursor(
    mut windows: Query<&mut Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    let mut window = windows.single_mut();

    if mouse.just_pressed(MouseButton::Left) {
        window.cursor_options.visible = false;
        window.cursor_options.grab_mode = CursorGrabMode::Locked;
    }

    if key.just_pressed(KeyCode::Escape) {
        window.cursor_options.visible = true;
        window.cursor_options.grab_mode = CursorGrabMode::None;
    }
}

/// Moves the player entity using WASD / Space / Left-Shift.
fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut player_query: Query<(&mut Transform, &MovementSpeed), With<Player>>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<Player>)>,
) {
    let Ok((mut transform, speed)) = player_query.get_single_mut() else {
        return;
    };

    let Ok(camera_transform) = camera_query.get_single() else {
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
    mut mouse_motion: EventReader<MouseMotion>,
    mut camera_query: Query<
        (&mut Transform, &mut CameraRotation, &MouseSensitivity),
        With<Camera3d>,
    >,
    windows: Query<&Window>,
) {
    let window = windows.single();

    // Only process mouse movement if cursor is grabbed
    if window.cursor_options.grab_mode != CursorGrabMode::Locked {
        return;
    }

    let Ok((mut transform, mut rotation, sensitivity)) = camera_query.get_single_mut() else {
        return;
    };

    for ev in mouse_motion.read() {
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
}

/// Syncs the camera position with the player position.
fn sync_camera_to_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };

    let Ok(mut camera_transform) = camera_query.get_single_mut() else {
        return;
    };

    camera_transform.translation = player_transform.translation;
}

/// Bevy plugin that registers player types and spawns the player on startup.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Player>()
            .register_type::<MovementSpeed>()
            .register_type::<MouseSensitivity>()
            .register_type::<CameraRotation>()
            .add_systems(Startup, (spawn_player, setup_camera))
            .add_systems(
                Update,
                (
                    grab_cursor,
                    mouse_look,
                    player_movement,
                    sync_camera_to_player,
                )
                    .chain(),
            );
    }
}
