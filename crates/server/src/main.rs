use bevy::{diagnostic::DiagnosticsPlugin, prelude::*};
use dd40_chunk_storage::plugin::DiskStoragePlugin;
use dd40_core::{common::log_plugin, plugin::CorePlugin};
use dd40_network::{
    ServerNetworkPlugin,
    server::connection::{DDServer, LinkConditionerConfig, RecvLinkConditioner},
    shared::connection::SHARED_SETTINGS,
};
use dd40_world::{WorldPlugin, generators::flat::FlatWorldGenerator};

fn main() {
    App::new()
        // MinimalPlugins gives us ECS, scheduling, and time – but no window or rendering.
        .add_plugins(MinimalPlugins)
        .add_plugins(log_plugin())
        .add_plugins(DiagnosticsPlugin)
        .add_plugins((
            CorePlugin,
            DiskStoragePlugin::new("world_data/chunks"),
            WorldPlugin::new(FlatWorldGenerator::default()),
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
