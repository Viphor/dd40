use bevy::prelude::*;
use dd40_core::prelude::LoadingTracker;

/// The loading tracker key used by the network client while waiting for the
/// initial server connection to be established.
pub const LOADING_KEY_SERVER_CONNECTION: &str = "network:server_connection";

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

pub fn remove_connection_loading_item(mut tracker: ResMut<LoadingTracker>) {
    if tracker.remove(LOADING_KEY_SERVER_CONNECTION) {
        info!(
            "ClientNetworkPlugin: server connection established — cleared \"{}\"",
            LOADING_KEY_SERVER_CONNECTION
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
