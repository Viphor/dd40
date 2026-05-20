use bevy::prelude::*;
use dd40_core::block::BlockDataTypeRegistry;
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
#[derive(Debug, Default)]
pub(crate) struct UpdateOutcome {
    /// Whether the chunk was found and the update was applied successfully.
    pub applied: bool,
    /// `Some(local_version)` if the caller should re-issue a `RequestChunk`
    /// because the version did not match, or because a cell-data entry
    /// referenced an unknown [`BlockData`] type.
    pub resync: Option<u64>,
    /// Predicted changes the server did not honour. The caller emits a
    /// `PredictionRejected` for each.
    pub rejected: Vec<ChunkChange>,
    /// Block-level changes successfully applied; forwarded as part of the
    /// emitted [`ChunkChanged`] message so renderer/audio/etc see them.
    pub applied_changes: Vec<ChunkChange>,
    /// Cell-data changes successfully applied; forwarded as part of the
    /// emitted [`ChunkChanged`] message.
    pub applied_cell_data: Vec<CellDataChange>,
}

/// Reconcile a [`ChunkUpdate`] against a chunk in the cache.
///
/// See [`apply_chunk_updates`] for the high-level rules.
pub(crate) fn reconcile_chunk_update(
    cache: &mut ChunkCache,
    registry: &BlockDataTypeRegistry,
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
            ..Default::default()
        };
    }

    let block_changes: Vec<ChunkChange> = update.changes.iter().copied().map(Into::into).collect();

    let mut cell_data_changes: Vec<CellDataChange> =
        Vec::with_capacity(update.cell_data_changes.len());
    for wire in &update.cell_data_changes {
        match wire.clone().decode(registry) {
            Ok(c) => cell_data_changes.push(c),
            Err(e) => {
                warn!(
                    "Failed to decode cell-data change for chunk {}: {} — re-requesting",
                    update.pos, e
                );
                return UpdateOutcome {
                    applied: false,
                    resync: Some(local_version),
                    ..Default::default()
                };
            }
        }
    }

    let predicted = chunk.take_predicted();
    for entry in predicted.iter().rev() {
        chunk.rollback_to(entry.change.local(), entry.prior);
    }

    if !chunk.apply_confirmed_changes(update.base_version, &block_changes) {
        error!(
            "apply_confirmed_changes refused for chunk {} at version {}",
            update.pos, update.base_version
        );
        return UpdateOutcome::default();
    }

    let post_block_version = chunk.version();
    if !chunk.apply_confirmed_cell_data_changes(post_block_version, cell_data_changes.clone()) {
        error!(
            "apply_confirmed_cell_data_changes refused for chunk {} at version {}",
            update.pos, post_block_version
        );
        return UpdateOutcome::default();
    }

    let rejected = predicted
        .iter()
        .filter(|entry| !block_changes.iter().any(|c| c == &entry.change))
        .map(|entry| entry.change)
        .collect();

    cache.mark_dirty(update.pos);

    UpdateOutcome {
        applied: true,
        resync: None,
        rejected,
        applied_changes: block_changes,
        applied_cell_data: cell_data_changes,
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
    registry: Res<BlockDataTypeRegistry>,
    mut changed: MessageWriter<ChunkChanged>,
    mut rejected: MessageWriter<PredictionRejected>,
    mut requests: MessageWriter<RequestChunk>,
) {
    for update in receiver.receive() {
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);

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
            changes: outcome.applied_changes,
            cell_data_changes: outcome.applied_cell_data,
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
        ChunkPos::new(0, 0, 0)
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

    /// Build a `ChunkUpdate` from runtime `ChunkChange`s and an empty
    /// cell-data delta. Keeps the existing tests readable.
    fn update_from(base_version: u64, new_version: u64, changes: Vec<ChunkChange>) -> ChunkUpdate {
        ChunkUpdate {
            pos: pos(),
            base_version,
            changes: changes.into_iter().map(Into::into).collect(),
            cell_data_changes: Vec::new(),
            new_version,
        }
    }

    #[test]
    fn missing_chunk_is_ignored() {
        let mut cache = ChunkCache::new();
        let registry = BlockDataTypeRegistry::default();
        let update = update_from(1, 2, vec![ChunkChange::new_remove(cell(0))]);
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);
        assert!(!outcome.applied);
        assert!(outcome.resync.is_none());
    }

    #[test]
    fn version_mismatch_requests_resync() {
        let mut cache = cache_with_chunk(5);
        let registry = BlockDataTypeRegistry::default();
        let update = update_from(3, 4, vec![]);
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);
        assert!(!outcome.applied);
        assert_eq!(outcome.resync, Some(5));
        assert!(outcome.rejected.is_empty());
    }

    #[test]
    fn matched_prediction_is_silently_confirmed() {
        let mut cache = cache_with_chunk(1);
        let registry = BlockDataTypeRegistry::default();
        let change = ChunkChange::new_place(cell(0), BlockId(7));
        assert!(cache.push_predicted(pos(), change));

        let update = update_from(1, 2, vec![change]);
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);

        assert!(outcome.applied);
        assert!(outcome.rejected.is_empty());
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.version(), 2);
        assert!(chunk.predicted().is_empty());
    }

    #[test]
    fn unmatched_prediction_is_rolled_back_and_rejected() {
        let mut cache = cache_with_chunk(1);
        let registry = BlockDataTypeRegistry::default();
        let predicted = ChunkChange::new_place(cell(0), BlockId(7));
        assert!(cache.push_predicted(pos(), predicted));

        let other = ChunkChange::new_place(cell(1), BlockId(8));
        let update = update_from(1, 2, vec![other]);
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);

        assert!(outcome.applied);
        assert_eq!(outcome.rejected, vec![predicted]);
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.get_local(cell(0)).block_id, BlockId::AIR);
        assert_eq!(chunk.get_local(cell(1)).block_id, BlockId(8));
        assert_eq!(chunk.version(), 2);
    }

    #[test]
    fn multiple_predictions_same_cell_roll_back_to_original() {
        let mut cache = cache_with_chunk(1);
        let registry = BlockDataTypeRegistry::default();
        {
            let chunk = cache.get_mut(&pos()).unwrap();
            chunk.set_local(cell(0), dd40_core::block::Block::new(BlockId(99)));
        }

        let p1 = ChunkChange::new_replace(cell(0), BlockId(1));
        let p2 = ChunkChange::new_replace(cell(0), BlockId(2));
        assert!(cache.push_predicted(pos(), p1));
        assert!(cache.push_predicted(pos(), p2));

        let confirmed = ChunkChange::new_place(cell(5), BlockId(50));
        let update = update_from(1, 2, vec![confirmed]);
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);

        assert!(outcome.applied);
        assert_eq!(outcome.rejected, vec![p1, p2]);
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.get_local(cell(0)).block_id, BlockId(99));
        assert_eq!(chunk.get_local(cell(5)).block_id, BlockId(50));
    }

    // ---- Cell-data wire round-trip ----

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct ChestState {
        slots: u32,
        label: String,
    }

    impl dd40_core::block::BlockData for ChestState {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn dd40_core::block::BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn cell_data_set_survives_wire_roundtrip() {
        use dd40_core::chunk::wire::{SerializableCellDataChange, SerializableChunkChange};

        let mut cache = cache_with_chunk(1);
        let mut registry = BlockDataTypeRegistry::new();
        assert!(registry.register::<ChestState>());

        let chest = ChestState {
            slots: 27,
            label: "Storage A".into(),
        };
        let runtime = CellDataChange::new_set(cell(2), chest.clone());
        let wire = SerializableCellDataChange::try_from(&runtime).expect("encode");

        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: Vec::<SerializableChunkChange>::new(),
            cell_data_changes: vec![wire],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &registry, &update);

        assert!(outcome.applied);
        assert_eq!(outcome.applied_cell_data.len(), 1);
        let chunk = cache.get(&pos()).unwrap();
        assert_eq!(chunk.version(), 2);
        let stored = chunk
            .cell_data::<ChestState>(cell(2))
            .expect("chest state present");
        assert_eq!(stored, &chest);
    }

    #[test]
    fn unknown_cell_data_type_triggers_resync() {
        use dd40_core::chunk::wire::{SerializableCellDataChange, SerializableChunkChange};

        let mut cache = cache_with_chunk(1);
        // Decoder registry does NOT know about ChestState. Encoder side
        // just needs the static type, so we don't need a separate
        // encoder registry.
        let decoder_registry = BlockDataTypeRegistry::new();

        let chest = ChestState {
            slots: 9,
            label: "X".into(),
        };
        let runtime = CellDataChange::new_set(cell(0), chest);
        let wire = SerializableCellDataChange::try_from(&runtime).expect("encode");

        let update = ChunkUpdate {
            pos: pos(),
            base_version: 1,
            changes: Vec::<SerializableChunkChange>::new(),
            cell_data_changes: vec![wire],
            new_version: 2,
        };
        let outcome = reconcile_chunk_update(&mut cache, &decoder_registry, &update);

        assert!(!outcome.applied);
        assert_eq!(outcome.resync, Some(1));
        // Chunk version unchanged because the decode failed before any state was touched.
        assert_eq!(cache.get(&pos()).unwrap().version(), 1);
    }
}
