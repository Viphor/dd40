//! Example demonstrating the debug UI system with FPS counter.
//!
//! This example shows:
//! - How to add the DebugUiPlugin to display debug information
//! - FPS counter with color-coded performance indicators
//! - A simple spinning cube to generate varied frame rates

use bevy::prelude::*;
use dd40_debug_ui::DebugUiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Debug UI Demo - FPS Counter".into(),
                resolution: (800, 600).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(DebugUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, rotate_cube)
        .run();
}

#[derive(Component)]
struct RotatingCube;

/// Sets up the 3D scene with a camera and a rotating cube.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Spawn a cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.0, 2.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.3),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
        RotatingCube,
    ));

    // Spawn a light
    commands.spawn((
        PointLight {
            intensity: 2_000_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // Spawn camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    info!("Debug UI Demo started");
    info!("The FPS counter is displayed in the top-left corner:");
    info!("  - Green: ≥60 FPS (good performance)");
    info!("  - Yellow: 30-59 FPS (moderate performance)");
    info!("  - Red: <30 FPS (low performance)");
}

/// Rotates the cube to generate some visual activity.
fn rotate_cube(time: Res<Time>, mut query: Query<&mut Transform, With<RotatingCube>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() * 0.5);
        transform.rotate_x(time.delta_secs() * 0.3);
    }
}
