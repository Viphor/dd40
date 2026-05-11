use bevy::prelude::*;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::chunk::events::{ChunkChanged, PredictionRejected};
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::protocol::{ChunkChannel, ChunkSnapshot, ChunkUpdate};

pub(crate) fn send_chunk_requests(
    mut requests: MessageReader<RequestChunk>,
    mut sender: Single<&mut MessageSender<RequestChunk>>,
) {
    for request in requests.read() {
        trace!("Requesting chunk at {}", request.pos);
        sender.send::<ChunkChannel>(request.clone());
    }
}

/// Reads [`ChunkSnapshot`] messages off the wire and forwards each as a
/// local [`ChunkReady`] so the existing `chunk_ready_listener` inserts the
/// chunk wholesale into [`ChunkCache`]. Used for both initial loads and
/// snapshot-fallback recoveries.
pub(crate) fn receive_chunk_data(
    mut ready: MessageWriter<ChunkReady>,
    mut receiver: Single<&mut MessageReceiver<ChunkSnapshot>>,
) {
    for snapshot in receiver.receive() {
        let pos = snapshot.chunk.position();
        trace!("Received chunk snapshot at {}", pos);
        ready.write(ChunkReady {
            chunk: snapshot.chunk,
        });
    }
}

/// Outcome of reconciling a single [`ChunkUpdate`] against a chunk's
/// predicted queue. Pure data so the apply system stays trivially testable
/// without spinning up a lightyear connection.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UpdateOutcome {
    /// Whether the chunk was found and the update was applied successfully.
    pub applied: bool,
    /// `Some(local_version)` if the caller should re-issue a `RequestChunk`
    /// because the version did not match.
    pub resync: Option<u64>,
    /// Predicted changes the server did not honour. The caller emits a
    /// `PredictionRejected` for each.
    pub rejected: Vec<ChunkChange>,
}

/// Reconcile a [`ChunkUpdate`] against a chunk in the cache.
///
/// See [`apply_chunk_updates`] for the high-level rules.
pub(crate) fn reconcile_chunk_update(
    cache: &mut ChunkCache,
    update: &ChunkUpdate,
) -> UpdateOutcome {
    let Some(chunk) = cache.get_mut(&update.pos) else {
        return UpdateOutcome::default();
    };

    let local_version = chunk.version();
    if update.base_version != local_version {
        return UpdateOutcome {
            applied: false,
            resync: Some(local_version),
            rejected: Vec::new(),
        };
    }

    let predicted = chunk.take_predicted();
    for entry in predicted.iter().rev() {
        chunk.rollback_to(entry.change.local(), entry.prior);
    }

    if !chunk.apply_confirmed_changes(update.base_version, &update.changes) {
        error!(
            "apply_confirmed_changes refused for chunk {} at version {}",
            update.pos, update.base_version
        );
        return UpdateOutcome::default();
    }

    let rejected = predicted
        .iter()
        .filter(|entry| !update.changes.iter().any(|c| c == &entry.change))
        .map(|entry| entry.change)
        .collect();

    cache.mark_dirty(update.pos);

    UpdateOutcome {
        applied: true,
        resync: None,
        rejected,
    }
}

/// Receives [`ChunkUpdate`] deltas from the server and reconciles them with
/// any locally-predicted changes on each affected chunk.
///
/// Reconciliation rules:
///
/// - `update.base_version == local_version`: the client is in sync with the
///   server's pre-delta state. Predicted changes are rolled back (in
///   reverse order so their `prior` chain reproduces the original cell
///   values), the confirmed delta is applied, predictions matching a
///   confirmed change are dropped silently, and the rest fire
///   [`PredictionRejected`]. A local [`ChunkChanged`] is emitted so the
///   renderer remeshes.
/// - `update.base_version != local_version`: the client and server are out
///   of sync. The delta is dropped and a [`RequestChunk`] is issued so the
///   server can reply with either a catch-up `ChunkUpdate` or a full
///   snapshot.
///
/// Updates targeting chunks not currently in the cache are ignored — those
/// chunks were evicted (or never loaded) and will be re-fetched fresh if
/// the player approaches them again.
pub(crate) fn apply_chunk_updates(
    mut receiver: Single<&mut MessageReceiver<ChunkUpdate>>,
    mut cache: ResMut<ChunkCache>,
    mut changed: MessageWriter<ChunkChanged>,
    mut rejected: MessageWriter<PredictionRejected>,
    mut requests: MessageWriter<RequestChunk>,
) {
    for update in receiver.receive() {
        let outcome = reconcile_chunk_update(&mut cache, &update);

        if let Some(local_version) = outcome.resync {
            warn!(
                "ChunkUpdate base_version {} != local {} for chunk {} — re-requesting",
                update.base_version, local_version, update.pos
            );
            requests.write(RequestChunk {
                pos: update.pos,
                current_version: local_version,
            });
            continue;
        }

        if !outcome.applied {
            continue;
        }

        for change in outcome.rejected {
            rejected.write(PredictionRejected {
                pos: update.pos,
                change,
            });
        }

        changed.write(ChunkChanged {
            pos: update.pos,
            changes: update.changes.clone(),
            new_version: update.new_version,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockId;
    use dd40_core::chunk::{BlockLocal, Chunk, ChunkPos};

    fn pos() -> ChunkPos {
        ChunkPos::new(0, 0)
    }

    fn cell(x: u8) -> BlockLocal {
        BlockLocal::new(x, 0, 0)
    }

    fn cache_with_chunk(version: u64) -> ChunkCache {
        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(pos());
        chunk.set_version(version);
        cache.insert(chunk);
        cache
    }

    #[test]
    fn missing_chunk_is_ignored() {
        let mut cache = ChunkCache::new();
        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: vec![ChunkChange::new_remove(cell(0))],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &update);
        assert_eq!(outcome, UpdateOutcome::default());
    }

    #[test]
    fn version_mismatch_requests_resync() {
        let mut cache = cache_with_chunk(5);
        let update = ChunkUpdate {
            pos: pos(),
            base_version: 3,
            changes: vec![],
            new_version: 4,
        };
        let outcome = reconcile_chunk_update(&mut cache, &update);
        assert!(!outcome.applied);
        assert_eq!(outcome.resync, Some(5));
        assert!(outcome.rejected.is_empty());
    }

    #[test]
    fn matched_prediction_is_silently_confirmed() {
        let mut cache = cache_with_chunk(1);
        let change = ChunkChange::new_place(cell(0), BlockId(7));
        assert!(cache.push_predicted(pos(), change));

        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: vec![change],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &update);

        assert!(outcome.applied);
        assert!(outcome.rejected.is_empty());
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.version(), 2);
        assert!(chunk.predicted().is_empty());
    }

    #[test]
    fn unmatched_prediction_is_rolled_back_and_rejected() {
        let mut cache = cache_with_chunk(1);
        let predicted = ChunkChange::new_place(cell(0), BlockId(7));
        assert!(cache.push_predicted(pos(), predicted));

        let other = ChunkChange::new_place(cell(1), BlockId(8));
        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: vec![other],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &update);

        assert!(outcome.applied);
        assert_eq!(outcome.rejected, vec![predicted]);
        let chunk = cache.get(&pos()).unwrap();
        // Predicted cell rolled back to air; confirmed cell holds new block.
        assert_eq!(chunk.get_local(cell(0)).block_id, BlockId::AIR);
        assert_eq!(chunk.get_local(cell(1)).block_id, BlockId(8));
        assert_eq!(chunk.version(), 2);
    }

    #[test]
    fn multiple_predictions_same_cell_roll_back_to_original() {
        let mut cache = cache_with_chunk(1);
        // Pre-populate the cell with a known non-air block we can roll back to.
        {
            let chunk = cache.get_mut(&pos()).unwrap();
            chunk.set_local(cell(0), dd40_core::block::Block::new(BlockId(99)));
        }

        let p1 = ChunkChange::new_replace(cell(0), BlockId(1));
        let p2 = ChunkChange::new_replace(cell(0), BlockId(2));
        assert!(cache.push_predicted(pos(), p1));
        assert!(cache.push_predicted(pos(), p2));

        // Server confirms a totally different cell — both predictions are rejected.
        let confirmed = ChunkChange::new_place(cell(5), BlockId(50));
        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: vec![confirmed],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &update);

        assert!(outcome.applied);
        assert_eq!(outcome.rejected, vec![p1, p2]);
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.get_local(cell(0)).block_id, BlockId(99));
        assert_eq!(chunk.get_local(cell(5)).block_id, BlockId(50));
    }
}
