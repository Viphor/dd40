//! Server-side spawn logic — resolves a player's starting position and
//! pre-streams the surrounding chunks when they connect.
//!
//! When lightyear adds a [`LinkOf`] component to a connection entity (i.e. a
//! client has completed its handshake) the [`send_spawn_location`] system
//! fires. It:
//!
//! 1. Resolves the player's spawn position from [`PlayerLocations`], falling
//!    back to [`WorldSpawnConfig::default_spawn`] for first-time players.
//! 2. Sends a [`PlayerSpawnLocation`] message to that client so it knows
//!    where to place the player entity and which chunks to expect.
//! 3. Queues 9 [`RequestChunk`] messages (3×3 grid centred on the spawn
//!    position) through the existing chunk pipeline so the server begins
//!    loading/generating them immediately.

use bevy::{platform::collections::HashMap, prelude::*};
use lightyear::prelude::PeerId;

// ============================================================================
// RESOURCES
// ============================================================================

/// Configures the world's default spawn location.
///
/// Inserted as a resource by [`crate::server::ServerNetworkPlugin`]. Override
/// it after adding the plugin to customise where new or unknown players appear.
///
/// # Example
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use dd40_network::server::spawn::WorldSpawnConfig;
/// # let mut app = App::new();
/// app.insert_resource(WorldSpawnConfig {
///     default_spawn: Vec3::new(8.0, 80.0, 8.0),
/// });
/// ```
#[derive(Resource, Clone, Debug)]
pub struct WorldSpawnConfig {
    /// World-space position a player spawns at when no previous location is
    /// saved for them.
    pub default_spawn: Vec3,
}

impl Default for WorldSpawnConfig {
    fn default() -> Self {
        Self {
            default_spawn: Vec3::new(0.0, 74.0, 0.0),
        }
    }
}

/// Stores the last known world-space position for each connected (or recently
/// disconnected) player, keyed by lightyear [`PeerId`].
///
/// Call [`PlayerLocations::set`] from any system that tracks authoritative
/// player positions (e.g. one that reads replicated [`Transform`] components)
/// so that reconnecting players are restored to their last known location.
#[derive(Resource, Default, Debug)]
pub struct PlayerLocations {
    locations: HashMap<PeerId, Vec3>,
}

impl PlayerLocations {
    /// Records or updates the last known position for `peer_id`.
    pub fn set(&mut self, peer_id: PeerId, pos: Vec3) {
        self.locations.insert(peer_id, pos);
    }

    /// Returns the last known position for `peer_id`, or [`None`] if the
    /// player has never been seen before.
    pub fn get(&self, peer_id: PeerId) -> Option<Vec3> {
        self.locations.get(&peer_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_spawn_config_default() {
        let config = WorldSpawnConfig::default();
        assert_eq!(config.default_spawn, Vec3::new(0.0, 74.0, 0.0));
    }

    #[test]
    fn player_locations_unknown_returns_none() {
        let locations = PlayerLocations::default();
        assert!(locations.get(PeerId::Netcode(999)).is_none());
    }

    #[test]
    fn player_locations_set_and_get() {
        let mut locations = PlayerLocations::default();
        locations.set(PeerId::Netcode(42), Vec3::new(100.0, 64.0, 200.0));
        assert_eq!(
            locations.get(PeerId::Netcode(42)),
            Some(Vec3::new(100.0, 64.0, 200.0))
        );
    }

    #[test]
    fn player_locations_overwrite() {
        let mut locations = PlayerLocations::default();
        locations.set(PeerId::Netcode(1), Vec3::new(0.0, 64.0, 0.0));
        locations.set(PeerId::Netcode(1), Vec3::new(50.0, 70.0, 50.0));
        assert_eq!(
            locations.get(PeerId::Netcode(1)),
            Some(Vec3::new(50.0, 70.0, 50.0))
        );
    }
}
