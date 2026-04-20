use bevy::prelude::*;
use dd40_core::prelude::LoadingSet;
use lightyear::prelude::client::ClientPlugins;

use crate::{
    client::{
        block_placement::{receive_placed_blocks, send_place_requests},
        character::ClientCharacterPlugin,
        chunk_provider::{receive_chunk_data, send_chunk_requests},
        connection::{DDClient, connect, on_server_connected},
        loading::register_connection_loading_item,
        spawn::{
            SpawnChunkTimeout, receive_spawn_location, timeout_initial_chunks, track_initial_chunks,
        },
    },
    protocol::*,
    shared::{
        connection::{CLIENT_PORT, SERVER_ADDR},
        constants::tick_duration,
    },
};

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

        // Add character replication plugin (prediction, input buffering, position sync)
        app.add_plugins(ClientCharacterPlugin);

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
        app.add_systems(PreUpdate, send_chunk_requests);
        app.add_systems(PostUpdate, receive_chunk_data);
        app.add_systems(PostUpdate, receive_placed_blocks);
        app.add_systems(PostUpdate, send_place_requests);

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
                .after(receive_chunk_data),
        );

        info!("ClientNetworkPlugin added");
    }
}
