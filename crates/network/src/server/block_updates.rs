//! Server-side broadcast of authoritative chunk deltas.
//!
//! Reads the local [`ChunkChanged`] message stream emitted by the
//! chunk-authority commit pass and forwards each commit to every connected
//! client whose controlled character sits within
//! [`NetworkRenderDistance`] chunks (Chebyshev distance) of the changed
//! chunk.
//!
//! Clients outside that radius do not need to keep their cached chunk in
//! sync until they re-enter the radius — at which point their handshake
//! re-issues a `RequestChunk { current_version }` and the server replies
//! with either a catch-up [`ChunkUpdate`] or a [`ChunkSnapshot`].

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_core::prelude::*;
use dd40_physics_core::prelude::CharacterPosition;
use lightyear::prelude::{ControlledBy, MessageSender};

use crate::protocol::{BlockChannel, ChunkUpdate};

/// Default Chebyshev radius (in chunks) used when
/// `DD40_NETWORK__RENDER_DISTANCE` is unset or unparseable.
pub const DEFAULT_NETWORK_RENDER_DISTANCE: i32 = 8;

/// Render distance (Chebyshev radius in chunks) used to decide which
/// connected clients receive a [`ChunkUpdate`] broadcast.
///
/// Initialised once at startup from the `DD40_NETWORK__RENDER_DISTANCE`
/// environment variable; falls back to
/// [`DEFAULT_NETWORK_RENDER_DISTANCE`] when unset or unparseable.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NetworkRenderDistance(pub i32);

impl Default for NetworkRenderDistance {
    fn default() -> Self {
        let raw = match std::env::var("DD40_NETWORK__RENDER_DISTANCE") {
            Ok(v) => v,
            Err(_) => return Self(DEFAULT_NETWORK_RENDER_DISTANCE),
        };
        match raw.trim().parse::<i32>() {
            Ok(n) if n >= 0 => Self(n),
            Ok(n) => {
                warn!(
                    "DD40_NETWORK__RENDER_DISTANCE={n} is negative; falling back to {}",
                    DEFAULT_NETWORK_RENDER_DISTANCE
                );
                Self(DEFAULT_NETWORK_RENDER_DISTANCE)
            }
            Err(e) => {
                warn!(
                    "DD40_NETWORK__RENDER_DISTANCE={raw:?} is not an i32 ({e}); falling back to {}",
                    DEFAULT_NETWORK_RENDER_DISTANCE
                );
                Self(DEFAULT_NETWORK_RENDER_DISTANCE)
            }
        }
    }
}

/// Chebyshev distance between two chunks on the XZ plane.
#[inline]
fn chebyshev(a: ChunkPos, b: ChunkPos) -> i32 {
    (a.x - b.x).abs().max((a.z - b.z).abs())
}

/// Reads [`ChunkChanged`] commits from the authority and forwards each to
/// every connected client whose character is within
/// [`NetworkRenderDistance`] chunks of the change.
///
/// `base_version` is computed as `new_version - changes.len()` — the
/// commit pass bumps the version exactly once per accepted change.
pub(crate) fn broadcast_chunk_updates(
    mut reader: MessageReader<ChunkChanged>,
    render_distance: Res<NetworkRenderDistance>,
    characters: Query<(&CharacterPosition, &ControlledBy), With<Character>>,
    mut senders: Query<(Entity, &mut MessageSender<ChunkUpdate>)>,
) {
    let updates: Vec<ChunkChanged> = reader.read().cloned().collect();
    if updates.is_empty() {
        return;
    }

    // Precompute each connection's player chunk position once.
    let mut conn_chunks: HashMap<Entity, ChunkPos> = HashMap::new();
    for (pos, controlled) in &characters {
        conn_chunks.insert(controlled.owner, ChunkPos::from(&pos.0));
    }

    let radius = render_distance.0;

    for (conn_entity, mut sender) in &mut senders {
        let Some(player_chunk) = conn_chunks.get(&conn_entity).copied() else {
            continue;
        };
        for update in &updates {
            if chebyshev(player_chunk, update.pos) > radius {
                continue;
            }
            let len = update.changes.len() as u64;
            let base_version = update.new_version.saturating_sub(len);
            sender.send::<BlockChannel>(ChunkUpdate {
                pos: update.pos,
                base_version,
                changes: update.changes.clone(),
                new_version: update.new_version,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(x: i32, z: i32) -> ChunkPos {
        ChunkPos::new(x, z)
    }

    #[test]
    fn chebyshev_zero_for_same_chunk() {
        assert_eq!(chebyshev(cp(3, -2), cp(3, -2)), 0);
    }

    #[test]
    fn chebyshev_uses_max_of_axes() {
        assert_eq!(chebyshev(cp(0, 0), cp(5, 2)), 5);
        assert_eq!(chebyshev(cp(0, 0), cp(-1, 7)), 7);
    }

    #[test]
    fn render_distance_default_when_env_unset() {
        // SAFETY: tests run single-threaded for this crate by default;
        // remove the variable to exercise the unset branch.
        // Intentionally avoid setting any value — rely on absence.
        unsafe { std::env::remove_var("DD40_NETWORK__RENDER_DISTANCE") };
        let d = NetworkRenderDistance::default();
        assert_eq!(d.0, DEFAULT_NETWORK_RENDER_DISTANCE);
    }
}
