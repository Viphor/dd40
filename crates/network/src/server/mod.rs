use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;
use dd40_core::prelude::*;
use lightyear::prelude::server::ServerPlugins;

use crate::{
    connection::server::{DDServer, start},
    constants::tick_duration,
    protocol::*,
    server::{
        block_placement::receive_place_requests,
        chunk_provider::{receive_chunk_requests, send_chunk_data},
        chunk_requests::{ChunkRequests, add_message_handlers},
    },
};

pub mod block_placement;
pub mod chunk_provider;
pub mod chunk_requests;

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

        let _server = app.world_mut().spawn(self.0.clone()).id();
        app.add_systems(Startup, start);

        // Add communication systems
        app.register_type::<ChunkRequests>()
            .add_observer(add_message_handlers)
            .add_systems(Update, receive_chunk_requests)
            .add_systems(PostUpdate, send_chunk_data);

        // Add server systems
        app.add_systems(Update, placeholder_server_tick);

        // Process incoming place-block requests from clients and broadcast results.
        app.add_systems(PostUpdate, receive_place_requests);

        // Add observers for block events
        app.add_systems(PostUpdate, log_block_removed);
        app.add_systems(PostUpdate, log_block_changed);
    }
}

/// Placeholder system that runs every frame.
///
/// Replace this with actual server logic once lightyear is integrated.
fn placeholder_server_tick(_time: Res<Time>) {
    // Placeholder - implement actual server logic here
}

/// Observer that logs when a block is removed.
///
/// In a full implementation, this would broadcast the event to all clients.
fn log_block_removed(mut messages: MessageReader<BlockRemoved>) {
    for message in messages.read() {
        debug!(
            "Block removed at ({}, {}, {}) (not broadcasted - networking not implemented)",
            message.pos.x, message.pos.y, message.pos.z
        );
    }
}

/// Observer that logs when a block changes.
///
/// In a full implementation, this would broadcast the event to all clients.
fn log_block_changed(mut messages: MessageReader<BlockChanged>) {
    for message in messages.read() {
        debug!(
            "Block changed at ({}, {}, {}) from {:?} to {:?} (not broadcasted - networking not implemented)",
            message.pos.x, message.pos.y, message.pos.z, message.old_block_id, message.new_block_id
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
        app.add_plugins(ServerNetworkPlugin(DDServer::new(6969)));
        // Plugin should add successfully
        assert!(app.is_plugin_added::<ServerNetworkPlugin>());
    }
}
