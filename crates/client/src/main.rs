use bevy::prelude::*;
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::FilterQueryInspectorPlugin};
use dd40_core::{character::Character, common::log_plugin, plugin::CorePlugin};
use dd40_debug_ui::DebugUiPlugin;
use dd40_gui::plugin::GuiPlugin;
use dd40_network::ClientNetworkPlugin;
use dd40_player::PlayerInputPlugin;
use dd40_renderer::RendererPlugin;

fn main() {
    let mut app = App::new();

    app.add_plugins(
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
    .add_plugins((
        CorePlugin,
        PlayerInputPlugin,
        DebugUiPlugin,
        ClientNetworkPlugin,
        RendererPlugin,
        GuiPlugin,
    ))
    .add_plugins(EguiPlugin::default())
    .add_plugins(FilterQueryInspectorPlugin::<With<Character>>::default())
    .add_systems(Startup, setup);

    #[cfg(feature = "debug_network")]
    app.add_plugins(lightyear_ui::prelude::DebugUIPlugin);

    app.run();
}

/// Adds ambient lighting.
fn setup(mut ambient: ResMut<GlobalAmbientLight>) {
    ambient.brightness = 1000.0;
}
