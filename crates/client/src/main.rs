use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use dd40_core::{common::log_plugin, plugin::CorePlugin};
use dd40_debug_ui::DebugUiPlugin;
use dd40_network::ClientNetworkPlugin;
use dd40_player::PlayerPlugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Dirt Displacer 40".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(log_plugin()),
        )
        .add_plugins((CorePlugin, PlayerPlugin, DebugUiPlugin, ClientNetworkPlugin))
        .add_plugins(EguiPlugin::default())
        //.add_plugins(WorldInspectorPlugin::new())
        .add_systems(Startup, setup)
        .run();
}

/// Adds ambient lighting.
fn setup(mut ambient: ResMut<GlobalAmbientLight>) {
    ambient.brightness = 1000.0;
}
