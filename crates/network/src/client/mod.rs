use bevy::prelude::*;
use dd40_core::prelude::{BlockPlaced, ChunkReady, LoadingSet, LoadingTracker, RequestChunk};
use lightyear::prelude::{
    MessageReceiver, MessageSender,
    client::{ClientPlugins, Connected},
};

pub mod block_placement;
pub mod chunk_provider;
pub mod spawn;

use crate::{
    client::spawn::{
        RequestSpawnEvent, SpawnChunkTimeout, on_ready_to_request_spawn, receive_spawn_location,
        timeout_initial_chunks, track_initial_chunks,
    },
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

// ============================================================================
// PLUGIN
// ============================================================================

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
/// During [`AppState::Loading`] this plugin holds two keys in [`LoadingTracker`]:
///
/// 1. `"network:server_connection"` — cleared once the lightyear handshake
///    completes and the server sends a [`PlayerSpawnLocation`] message.
/// 2. `"network:initial_chunks"` — registered when [`PlayerSpawnLocation`]
///    arrives and cleared once all 9 chunks in the 3×3 spawn grid have been
///    received, or when [`SpawnChunkTimeout`] fires (whichever comes first).
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

        // Default timeout resource — can be overridden after plugin insertion.
        app.init_resource::<SpawnChunkTimeout>();

        // Register the server-connection loading item during startup.
        app.add_systems(Startup, register_connection_loading_item.in_set(LoadingSet));

        app.add_systems(Startup, connect);

        // Clear the server-connection gate as soon as the lightyear handshake
        // completes and attach the required message components.
        app.add_observer(on_server_connected);

        // Communication systems.
        app.add_systems(PreUpdate, chunk_provider::send_chunk_requests);
        app.add_systems(PostUpdate, chunk_provider::receive_chunk_data);
        app.add_systems(PostUpdate, block_placement::receive_placed_blocks);
        app.add_systems(PostUpdate, block_placement::send_place_requests);
        app.add_observer(on_ready_to_request_spawn);

        // Spawn-location and chunk-tracking systems run after chunk data has
        // been forwarded so notifications are written before we drain them.
        app.add_systems(
            PostUpdate,
            (
                receive_spawn_location,
                track_initial_chunks,
                timeout_initial_chunks,
            )
                .chain()
                .after(chunk_provider::receive_chunk_data),
        );

        info!("ClientNetworkPlugin added");
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Startup system that registers the server-connection loading item.
///
/// Runs inside [`LoadingSet`] so it is ordered before world-generation and
/// other loading registrations.
fn register_connection_loading_item(mut tracker: ResMut<LoadingTracker>) {
    tracker.add(LOADING_KEY_SERVER_CONNECTION, "Connecting to server…");
    info!(
        "ClientNetworkPlugin: waiting on \"{}\"",
        LOADING_KEY_SERVER_CONNECTION
    );
}

/// Observer that fires when lightyear adds the [`Connected`] component to the
/// client entity, i.e. when the server handshake completes.
///
/// Attaches all required [`MessageSender`] and [`MessageReceiver`] components
/// to the connection entity and clears the `"network:server_connection"` gate.
///
/// The `"network:initial_chunks"` gate is registered later, when the server
/// sends [`PlayerSpawnLocation`], so that the timeout only starts counting
/// once we actually have something to wait for.
fn on_server_connected(
    trigger: On<Add, Connected>,
    mut commands: Commands,
    mut tracker: ResMut<LoadingTracker>,
) {
    let entity = trigger.entity;

    commands.entity(entity).insert((
        MessageSender::<RequestSpawn>::default(),
        MessageSender::<RequestChunk>::default(),
        MessageSender::<PlaceBlockRequest>::default(),
        MessageReceiver::<ChunkReady>::default(),
        MessageReceiver::<BlockPlaced>::default(),
        MessageReceiver::<PlayerSpawnLocation>::default(),
        Name::new("ServerConnection"),
    ));

    if tracker.remove(LOADING_KEY_SERVER_CONNECTION) {
        info!(
            "ClientNetworkPlugin: server connection established — cleared \"{}\"",
            LOADING_KEY_SERVER_CONNECTION
        );
    }
    commands.trigger(RequestSpawnEvent);
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_key_server_connection_is_stable() {
        assert_eq!(LOADING_KEY_SERVER_CONNECTION, "network:server_connection");
    }
}
