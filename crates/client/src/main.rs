use bevy::prelude::*;
use dd40_character_gui::plugin::CharacterGuiPlugin;
use dd40_character_interaction::CharacterInteractionPlugin;
use dd40_core::{common::log_plugin, plugin::CorePlugin};
use dd40_debug_ui::DebugUiPlugin;
use dd40_gui::plugin::GuiPlugin;
use dd40_integration_character_physics::IntegrationCharacterPhysicsPlugin;
use dd40_inventory::{InventoryActiveItemPlugin, InventoryPlugin};
use dd40_inventory_gui::InventoryGuiPlugin;
use dd40_loose_item_render::LooseItemRenderPlugin;
use dd40_network::{ClientInventoryNetworkPlugin, ClientNetworkPlugin};
use dd40_physics::PhysicsPlugin;
use dd40_player_input::PlayerInputPlugin;
use dd40_renderer::RendererPlugin;
use dd40_texture_pack::{TexturePackConfig, TexturePackPlugin};
use dd40_vanilla_palette::VanillaPalettePlugin;

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
        PhysicsPlugin,
        IntegrationCharacterPhysicsPlugin,
        VanillaPalettePlugin,
        PlayerInputPlugin,
        CharacterInteractionPlugin,
        DebugUiPlugin,
        ClientNetworkPlugin,
        ClientInventoryNetworkPlugin,
        RendererPlugin,
        LooseItemRenderPlugin,
        GuiPlugin,
        CharacterGuiPlugin,
        InventoryPlugin,
    ))
    .add_plugins((InventoryActiveItemPlugin, InventoryGuiPlugin))
    .insert_resource(TexturePackConfig::with_search_path(
        "assets/resourcepacks/default",
    ))
    .add_plugins(TexturePackPlugin)
    .add_systems(Startup, setup);

    #[cfg(feature = "debug_network")]
    app.add_plugins(lightyear_ui::prelude::DebugUIPlugin);

    app.run();
}

/// Adds ambient lighting.
fn setup(mut ambient: ResMut<GlobalAmbientLight>) {
    ambient.brightness = 1000.0;
}
