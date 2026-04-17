use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;
use lightyear::prelude::server::ServerPlugins;

use crate::{
    protocol::*,
    server::{
        block_placement::receive_place_requests,
        character::ServerCharacterPlugin,
        chunk_provider::{receive_chunk_requests, send_chunk_data},
        chunk_requests::{ChunkRequests, add_message_handlers},
        connection::{DDServer, start},
        spawn::{PlayerLocations, WorldSpawnConfig, send_spawn_location},
    },
    shared::constants::tick_duration,
};

pub mod block_placement;
pub mod character;
pub mod chunk_provider;
pub mod chunk_requests;
pub mod connection;
pub mod spawn;

/// Plugin that sets up server-side networking.
///
/// This plugin handles:
/// - Accepting client connections
/// - Processing inputs from clients
/// - Authoritative game simulation
/// - Broadcasting state changes to clients
pub struct ServerNetworkPlugin(pub DDServer);

impl Plugin for ServerNetworkPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CorePlugin>() {
            panic!("ServerNetworkPlugin requires CorePlugin to be added to the app");
        }

        app.add_plugins(ServerPlugins {
            tick_duration: tick_duration(),
        });

        // Add protocol plugin (registers messages, components, inputs)
        app.add_plugins(ProtocolPlugin);

        // Add character replication plugin (spawn, input→controller, state sync)
        app.add_plugins(ServerCharacterPlugin);

        let _server = app.world_mut().spawn(self.0.clone()).id();
        app.add_systems(Startup, start);

        // Initialise spawn-handshake resources.
        app.init_resource::<WorldSpawnConfig>()
            .init_resource::<PlayerLocations>();

        // Add communication systems
        app.register_type::<ChunkRequests>()
            .add_observer(add_message_handlers)
            .add_systems(Update, receive_chunk_requests)
            .add_systems(Update, send_spawn_location)
            .add_systems(Update, send_chunk_data);

        // Process incoming place-block requests from clients and broadcast results.
        app.add_systems(Update, receive_place_requests);
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
        app.add_plugins(ServerNetworkPlugin(DDServer::new(6969)));
        // Plugin should add successfully
        assert!(app.is_plugin_added::<ServerNetworkPlugin>());
    }
}
