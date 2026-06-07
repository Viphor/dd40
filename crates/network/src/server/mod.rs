use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_config::RawConfig;
use dd40_core::chunk::ChunkAuthorityPlugin;
use dd40_core::plugin::CorePlugin;
use lightyear::prelude::server::ServerPlugins;

use crate::{
    protocol::*,
    server::{
        block_updates::{
            NetworkRenderDistance, broadcast_chunk_rejections, broadcast_chunk_updates,
        },
        character::ServerCharacterPlugin,
        chunk_provider::{receive_chunk_requests, send_chunk_data},
        chunk_requests::{ChunkRequests, add_message_handlers},
        config::NetworkConfig,
        connection::{DDServer, start},
        spawn::{PlayerLocations, WorldSpawnConfig},
    },
    shared::constants::tick_duration,
};

pub mod block_updates;
pub mod character;
pub mod chunk_provider;
pub mod chunk_requests;
pub mod config;
pub mod connection;
pub mod inventory;
pub mod loose_items;
pub mod spawn;
pub mod user;

/// Plugin that sets up server-side networking.
///
/// Configuration (port, private key, render distance) is read from
/// [`dd40_config::RawConfig`] at build time, so [`dd40_config::ConfigPlugin`]
/// must be added before this plugin.
#[derive(Default)]
pub struct ServerNetworkPlugin;

impl Plugin for ServerNetworkPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin, CharacterCorePlugin, ChunkAuthorityPlugin);

        app.add_plugins(ServerPlugins {
            tick_duration: tick_duration(),
        });

        // Add protocol plugin (registers messages, components, inputs)
        app.add_plugins(ProtocolPlugin);

        // Add character replication plugin (spawn, input→controller, state sync)
        app.add_plugins(ServerCharacterPlugin);

        let _server = app.world_mut().spawn(DDServer).id();
        app.add_systems(Startup, start);

        // Read render distance from config; fall back to the compiled-in default.
        let render_distance = app
            .world()
            .get_resource::<RawConfig>()
            .map(|r| r.section::<NetworkConfig>().render_distance)
            .unwrap_or(NetworkConfig::default().render_distance);
        info!(render_distance, "network render distance");

        // Initialise spawn-handshake resources.
        app.init_resource::<WorldSpawnConfig>()
            .init_resource::<PlayerLocations>()
            .insert_resource(NetworkRenderDistance(render_distance));

        // Add communication systems
        app.register_type::<ChunkRequests>()
            .add_observer(add_message_handlers)
            .add_systems(Update, receive_chunk_requests)
            .add_systems(Update, send_chunk_data)
            .add_systems(Update, broadcast_chunk_updates)
            .add_systems(Update, broadcast_chunk_rejections)
            .add_systems(
                PostUpdate,
                (
                    loose_items::add_loose_item_replication,
                    loose_items::sync_loose_item_position,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_builds() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CorePlugin);
        app.add_plugins(ServerNetworkPlugin);
        assert!(app.is_plugin_added::<ServerNetworkPlugin>());
    }
}
