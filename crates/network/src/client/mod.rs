use bevy::prelude::*;
use dd40_core::prelude::{LoadingSet, LoadingTracker};
use lightyear::prelude::client::{ClientPlugins, Connected};

pub mod chunk_provider;

use crate::{
    connection::{
        client::{DDClient, connect},
        shared::{CLIENT_PORT, SERVER_ADDR},
    },
    constants::tick_duration,
    protocol::*,
};

/// The loading tracker key used by the network client while waiting for the
/// initial server connection to be established.
pub const LOADING_KEY_SERVER_CONNECTION: &str = "network:server_connection";

/// Plugin that sets up client-side networking.
///
/// This plugin handles:
/// - Connection to the server
/// - Input collection and sending
/// - Message and component replication
/// - Client-side prediction (when implemented)
///
/// # Loading integration
///
/// During the [`AppState::Loading`] phase this plugin registers the
/// `"network:server_connection"` key with [`LoadingTracker`]. The key is
/// removed as soon as lightyear adds the [`Connected`] component to the client
/// entity, which signals a successful handshake with the server. The app will
/// not leave the `Loading` state until this (and every other registered key)
/// has been cleared.
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

        // Register the connection loading item so the Loading state is held
        // until the server handshake completes.
        app.add_systems(Startup, register_connection_loading_item.in_set(LoadingSet));

        app.add_systems(Startup, connect);

        // Observe the lightyear Connected component being added to the client
        // entity. When it fires the server handshake is complete and we can
        // clear the loading item.
        app.add_observer(on_server_connected);

        // Add communication systems
        app.add_systems(PreUpdate, chunk_provider::send_chunk_requests);
        app.add_systems(PostUpdate, chunk_provider::receive_chunk_data);

        // Add client systems
        app.add_systems(Update, placeholder_client_tick);

        info!("ClientNetworkPlugin added");
    }
}

/// Startup system that registers the server-connection loading item.
///
/// Runs inside [`LoadingSet`] so it is ordered correctly relative to other
/// loading registrations and world-generation systems.
fn register_connection_loading_item(mut tracker: ResMut<LoadingTracker>) {
    tracker.add(LOADING_KEY_SERVER_CONNECTION, "Connecting to server…");
    info!(
        "ClientNetworkPlugin: waiting on \"{}\"",
        LOADING_KEY_SERVER_CONNECTION
    );
}

/// Observer that fires when lightyear adds the [`Connected`] component to any
/// entity (i.e. when the client has successfully completed the server
/// handshake).
///
/// This clears the `"network:server_connection"` key from [`LoadingTracker`],
/// allowing the app to proceed to [`AppState::Playing`] once all other loading
/// items are also resolved.
fn on_server_connected(_trigger: On<Add, Connected>, mut tracker: ResMut<LoadingTracker>) {
    if tracker.remove(LOADING_KEY_SERVER_CONNECTION) {
        info!(
            "ClientNetworkPlugin: server connection established — cleared \"{}\"",
            LOADING_KEY_SERVER_CONNECTION
        );
    }
}

/// Placeholder system that runs every frame.
///
/// Replace this with actual client logic once the full lightyear integration
/// is complete.
fn placeholder_client_tick(_time: Res<Time>) {
    // Placeholder - implement actual client logic here
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_key_is_stable() {
        // Ensures the public constant is not accidentally changed, which would
        // silently break any external crate that removes it by name.
        assert_eq!(LOADING_KEY_SERVER_CONNECTION, "network:server_connection");
    }
}
