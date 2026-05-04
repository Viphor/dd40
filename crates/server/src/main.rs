use bevy::{diagnostic::DiagnosticsPlugin, prelude::*};
use dd40_chunk_storage::plugin::DiskStoragePlugin;
use dd40_core::{common::log_plugin, plugin::CorePlugin};
use dd40_integration_character_physics::IntegrationCharacterPhysicsPlugin;
use dd40_physics::PhysicsPlugin;
use dd40_network::{
    ServerNetworkPlugin,
    server::connection::{DDServer, LinkConditionerConfig, RecvLinkConditioner},
    shared::connection::SHARED_SETTINGS,
};
use dd40_vanilla_palette::{VanillaPalettePlugin, VanillaBlocks};
use dd40_world::{WorldPlugin, generators::flat::{FlatWorldGenerator, Layer}};

fn main() {
    App::new()
        // MinimalPlugins gives us ECS, scheduling, and time – but no window or rendering.
        .add_plugins(MinimalPlugins)
        .add_plugins(log_plugin())
        .add_plugins(DiagnosticsPlugin)
        .add_plugins((
            CorePlugin,
            PhysicsPlugin,
            IntegrationCharacterPhysicsPlugin,
            VanillaPalettePlugin,
            DiskStoragePlugin::new("world_data/chunks"),
            WorldPlugin::new(FlatWorldGenerator(vec![
                Layer { block_id: VanillaBlocks::STONE, height_range: 0..58 },
                Layer { block_id: VanillaBlocks::DIRT,  height_range: 58..62 },
                Layer { block_id: VanillaBlocks::GRASS, height_range: 62..63 },
            ])),
            ServerNetworkPlugin(DDServer {
                conditioner: Some(RecvLinkConditioner::new(
                    LinkConditionerConfig::average_condition(),
                )),
                port: 6969,
                shared: SHARED_SETTINGS,
            }),
        ))
        .add_systems(Update, server_tick)
        .run();
}

/// Placeholder server tick system – extend with network and game-logic code.
fn server_tick(_time: Res<Time>) {}
