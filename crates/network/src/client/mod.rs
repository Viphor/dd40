use bevy::prelude::*;
use lightyear::prelude::client::ClientPlugins;

pub mod chunk_provider;

use crate::{
    connection::{
        client::{DDClient, connect},
        shared::{CLIENT_PORT, SERVER_ADDR},
    },
    constants::tick_duration,
    protocol::*,
};

/// Plugin that sets up client-side networking.
///
/// This plugin handles:
/// - Connection to the server
/// - Input collection and sending
/// - Message and component replication
/// - Client-side prediction (when implemented)
pub struct ClientNetworkPlugin;

impl Plugin for ClientNetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ClientPlugins {
            tick_duration: tick_duration(),
        });

        // Add protocol plugin (registers messages, components, inputs)
        app.add_plugins(ProtocolPlugin);

        let _client = app
            .world_mut()
            .spawn(DDClient::new(CLIENT_PORT, SERVER_ADDR))
            .id();
        app.add_systems(Startup, connect);

        // Add communication systems
        app.add_systems(PreUpdate, chunk_provider::send_chunk_requests);
        app.add_systems(PostUpdate, chunk_provider::receive_chunk_data);

        // Add client systems
        app.add_systems(Update, placeholder_client_tick);

        info!("ClientNetworkPlugin added (skeleton implementation)");
    }
}

/// Placeholder system that runs every frame.
///
/// Replace this with actual client logic once lightyear is integrated.
fn placeholder_client_tick(_time: Res<Time>) {
    // Placeholder - implement actual client logic here
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_builds() {
        let mut app = App::new();
        app.add_plugins(ClientNetworkPlugin);
        // Plugin should add successfully
        assert!(app.is_plugin_added::<ClientNetworkPlugin>());
    }
}
