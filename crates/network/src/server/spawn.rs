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
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{
    protocol::{EventChannel, PlayerSpawnLocation, RequestSpawn},
    server::chunk_requests::ChunkRequests,
};

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
            default_spawn: Vec3::new(0.0, 174.0, 0.0),
        }
    }
}

/// Stores the last known world-space position for each connected (or recently
/// disconnected) player, keyed by lightyear client id.
///
/// Call [`PlayerLocations::set`] from any system that tracks authoritative
/// player positions (e.g. one that reads replicated [`Transform`] components)
/// so that reconnecting players are restored to their last known location.
#[derive(Resource, Default, Debug)]
pub struct PlayerLocations {
    locations: HashMap<u64, Vec3>,
}

impl PlayerLocations {
    /// Records or updates the last known position for `client_id`.
    pub fn set(&mut self, client_id: u64, pos: Vec3) {
        self.locations.insert(client_id, pos);
    }

    /// Returns the last known position for `client_id`, or [`None`] if the
    /// player has never been seen before.
    pub fn get(&self, client_id: u64) -> Option<Vec3> {
        self.locations.get(&client_id).copied()
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Sends a [`PlayerSpawnLocation`] and queues 9 initial [`RequestChunk`]
/// messages whenever a new client connection entity is ready.
///
/// This system runs every frame and processes all connection entities that
/// have a pending [`NewClientMarker`] component (inserted by
/// [`mark_new_clients`]). For each such entity it:
///
/// 1. Resolves the spawn position from [`PlayerLocations`] (falling back to
///    [`WorldSpawnConfig::default_spawn`]).
/// 2. Sends [`PlayerSpawnLocation`] to the client over [`EventChannel`].
/// 3. Pushes 9 [`RequestChunk`] messages into the shared pipeline so the
///    chunk provider starts loading/generating them right away. The
///    [`ChunkRequests`] set deduplicates positions so repeated calls for the
///    same chunk are harmless.
pub(crate) fn send_spawn_location(
    spawn_config: Res<WorldSpawnConfig>,
    player_locations: Res<PlayerLocations>,
    mut chunk_request_writer: MessageWriter<RequestChunk>,
    mut connections: Query<(
        &mut MessageReceiver<RequestSpawn>,
        &mut MessageSender<PlayerSpawnLocation>,
        &mut ChunkRequests,
    )>,
) {
    for (mut request, mut sender, mut chunk_requests) in connections.iter_mut() {
        let Some(RequestSpawn(client_id)) = request.receive().next() else {
            continue; // No spawn request from this client yet.
        };

        // 1. Resolve spawn position.
        let spawn_pos = player_locations
            .get(client_id)
            .unwrap_or(spawn_config.default_spawn);

        // 2. Derive the centre chunk from the spawn position.
        let centre = ChunkPos::from(&spawn_pos);

        debug!(
            "Sending spawn location to client {}: pos={:?}, centre_chunk={:?}",
            client_id, spawn_pos, centre,
        );

        // 3. Notify the client of its spawn position.
        sender.send::<EventChannel>(PlayerSpawnLocation {
            position: spawn_pos,
        });

        // 4. Queue the 3×3 grid of surrounding chunks.
        for dx in -1_i32..=1 {
            for dz in -1_i32..=1 {
                let pos = ChunkPos::new(centre.x + dx, centre.z + dz);
                if chunk_requests.insert(pos) {
                    // Guard against duplicates — ChunkRequests is a HashSet so
                    // `insert` returns false when the position was already tracked.
                    chunk_request_writer.write(RequestChunk { pos });
                }
            }
        }
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
        assert!(locations.get(999).is_none());
    }

    #[test]
    fn player_locations_set_and_get() {
        let mut locations = PlayerLocations::default();
        locations.set(42, Vec3::new(100.0, 64.0, 200.0));
        assert_eq!(locations.get(42), Some(Vec3::new(100.0, 64.0, 200.0)));
    }

    #[test]
    fn player_locations_overwrite() {
        let mut locations = PlayerLocations::default();
        locations.set(1, Vec3::new(0.0, 64.0, 0.0));
        locations.set(1, Vec3::new(50.0, 70.0, 50.0));
        assert_eq!(locations.get(1), Some(Vec3::new(50.0, 70.0, 50.0)));
    }

    #[test]
    fn centre_chunk_at_origin() {
        // Spawn at origin → centre chunk should be (0, 0).
        let pos = Vec3::ZERO;
        let centre = ChunkPos::new(
            (pos.x / CHUNK_SIZE_X as f32).floor() as i32,
            (pos.z / CHUNK_SIZE_Z as f32).floor() as i32,
        );
        assert_eq!(centre, ChunkPos::new(0, 0));
    }

    #[test]
    fn centre_chunk_at_negative_coords() {
        // Spawn at (-8, 64, -8) → still inside chunk (-1, -1).
        let pos = Vec3::new(-8.0, 64.0, -8.0);
        let centre = ChunkPos::new(
            (pos.x / CHUNK_SIZE_X as f32).floor() as i32,
            (pos.z / CHUNK_SIZE_Z as f32).floor() as i32,
        );
        assert_eq!(centre, ChunkPos::new(-1, -1));
    }

    #[test]
    fn centre_chunk_boundary() {
        // Spawn at exactly x=16 → chunk (1, 0).
        let pos = Vec3::new(16.0, 64.0, 0.0);
        let centre = ChunkPos::new(
            (pos.x / CHUNK_SIZE_X as f32).floor() as i32,
            (pos.z / CHUNK_SIZE_Z as f32).floor() as i32,
        );
        assert_eq!(centre, ChunkPos::new(1, 0));
    }

    #[test]
    fn three_by_three_grid_has_nine_entries() {
        let centre = ChunkPos::new(0, 0);
        let mut positions = Vec::new();
        for dx in -1_i32..=1 {
            for dz in -1_i32..=1 {
                positions.push(ChunkPos::new(centre.x + dx, centre.z + dz));
            }
        }
        assert_eq!(positions.len(), 9);
    }

    #[test]
    fn three_by_three_grid_covers_expected_positions() {
        let centre = ChunkPos::new(2, -3);
        let mut positions = std::collections::HashSet::new();
        for dx in -1_i32..=1 {
            for dz in -1_i32..=1 {
                positions.insert(ChunkPos::new(centre.x + dx, centre.z + dz));
            }
        }
        // All nine positions should be unique.
        assert_eq!(positions.len(), 9);
        assert!(positions.contains(&ChunkPos::new(1, -4)));
        assert!(positions.contains(&ChunkPos::new(2, -3)));
        assert!(positions.contains(&ChunkPos::new(3, -2)));
    }
}
