use bevy::prelude::*;
use dd40_core::prelude::LoadingTracker;

/// The loading tracker key used by the network client while waiting for the
/// initial server connection to be established.
pub const LOADING_KEY_SERVER_CONNECTION: &str = "network:server_connection";

/// The loading tracker key held while the client is waiting for the initial
/// 3×3 spawn chunks to arrive from the server.
///
/// Registered in [`LoadingTracker`] when a [`PlayerSpawnLocation`] message is
/// received and cleared once every chunk in the 3×3 grid has arrived — or
/// when [`SpawnChunkTimeout`] fires, whichever comes first.
pub const LOADING_KEY_INITIAL_CHUNKS: &str = "network:initial_chunks";

/// The loading tracker key used by the network client while waiting for the
/// server to send the player's spawn location.
///
/// Registered in [`LoadingTracker`] when the server connection is established and
/// cleared once a [`PlayerSpawnLocation`] message is received. This is separate
/// from [`LOADING_KEY_INITIAL_CHUNKS`] so that the client can show a different
/// loading message while waiting for the spawn location vs. waiting for the spawn chunks.
pub const LOADING_KEY_SPAWN_LOCATION: &str = "network:spawn_location";

/// Startup system that registers the server-connection loading item.
///
/// Runs inside [`LoadingSet`] so it is ordered before world-generation and
/// other loading registrations.
pub fn register_connection_loading_item(mut tracker: ResMut<LoadingTracker>) {
    tracker.add(LOADING_KEY_SERVER_CONNECTION, "Connecting to server…");
    info!(
        "ClientNetworkPlugin: waiting on \"{}\"",
        LOADING_KEY_SERVER_CONNECTION
    );
}

pub fn remove_connection_loading_item(tracker: &mut LoadingTracker) {
    if tracker.remove(LOADING_KEY_SERVER_CONNECTION) {
        info!(
            "ClientNetworkPlugin: server connection established — cleared \"{}\"",
            LOADING_KEY_SERVER_CONNECTION
        );
    }
}

pub fn register_spawn_location_loading_item(tracker: &mut LoadingTracker) {
    tracker.add(LOADING_KEY_SPAWN_LOCATION, "Waiting for spawn location…");
    info!(
        "ClientNetworkPlugin: waiting on \"{}\"",
        LOADING_KEY_SPAWN_LOCATION
    );
}

pub fn remove_spawn_location_loading_item(tracker: &mut LoadingTracker) {
    if tracker.remove(LOADING_KEY_SPAWN_LOCATION) {
        info!(
            "ClientNetworkPlugin: received spawn location — cleared \"{}\"",
            LOADING_KEY_SPAWN_LOCATION
        );
    }
}

pub fn register_initial_chunks_loading_item(tracker: &mut LoadingTracker) {
    tracker.add(LOADING_KEY_INITIAL_CHUNKS, "Loading spawn area…");
    info!(
        "ClientNetworkPlugin: waiting on \"{}\"",
        LOADING_KEY_INITIAL_CHUNKS
    );
}

pub fn remove_initial_chunks_loading_item(tracker: &mut LoadingTracker) {
    if tracker.remove(LOADING_KEY_INITIAL_CHUNKS) {
        info!(
            "ClientNetworkPlugin: initial spawn chunks loaded — cleared \"{}\"",
            LOADING_KEY_INITIAL_CHUNKS
        );
    }
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
