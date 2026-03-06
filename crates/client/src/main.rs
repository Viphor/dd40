use bevy::prelude::*;
use dd40_core::CorePlugin;
use dd40_debug_ui::DebugUiPlugin;
use dd40_player::PlayerPlugin;
use dd40_world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "dirt displacer 40".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((CorePlugin, WorldPlugin, PlayerPlugin, DebugUiPlugin))
        .add_systems(Startup, setup)
        .run();
}

/// Adds ambient lighting.
fn setup(mut ambient: ResMut<AmbientLight>) {
    ambient.brightness = 1000.0;
}
