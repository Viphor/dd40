use bevy::prelude::*;
use dd40_core::CorePlugin;
use dd40_player::{MovementSpeed, Player, PlayerPlugin};
use dd40_world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "dd40".into(),
                    ..default()
                }),
                ..default()
            }),
        )
        .add_plugins((CorePlugin, WorldPlugin, PlayerPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, player_movement)
        .run();
}

/// Spawns the 3-D camera attached to the player and adds ambient lighting.
fn setup(mut commands: Commands, mut ambient: ResMut<AmbientLight>) {
    ambient.brightness = 1000.0;

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(8.0, 70.0, 8.0).looking_at(Vec3::new(8.0, 64.0, 0.0), Vec3::Y),
    ));
}

/// Moves the player entity using WASD / Space / Left-Shift.
fn player_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &MovementSpeed), With<Player>>,
) {
    for (mut transform, speed) in &mut query {
        let mut direction = Vec3::ZERO;

        if keyboard.pressed(KeyCode::KeyW) {
            direction -= Vec3::Z;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction += Vec3::Z;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction -= Vec3::X;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction += Vec3::X;
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
}
