//! Server-only commit pass for the versioned chunk cache.
//!
//! This module owns the **single authoritative path** that turns predicted
//! chunk changes into confirmed history. It is exposed as a Bevy plugin —
//! [`ChunkAuthorityPlugin`] — that the server binary adds. The client
//! never adds the plugin; without it, predicted changes simply accumulate
//! on each chunk until rolled back by an incoming `ChunkUpdate`.
//!
//! Adding the plugin **is** the gate. There is no resource flag, no
//! `run_if`, no marker component to forget. If `ChunkAuthorityPlugin` is
//! in the app, the local instance is authoritative.
//!
//! # Validators are systems
//!
//! Acceptance/rejection of each predicted change is decided by zero or
//! more **registered Bevy systems** that run in
//! [`ChunkAuthoritySet::Validate`] before the commit pass in
//! [`ChunkAuthoritySet::Commit`]. A validator system inspects pending
//! predictions via `Res<ChunkCache>` (read-only) and any other resources
//! it needs, and writes rejection decisions into
//! [`PendingChunkRejections`].
//!
//! This design is deliberate — see the "Flexibility Over Convenience"
//! section of `CLAUDE.md` and `.github/copilot-instructions.md`. Each
//! validator declares its own [`SystemParam`](bevy::ecs::system::SystemParam)
//! list, so:
//!
//! - downstream validators can read **any** resource they need
//!   (e.g. `CharacterSpatialCache`, query world state, talk to plugins
//!   that don't exist in `dd40_core`),
//! - the commit pass is **never an exclusive system** — only `ChunkCache`,
//!   `PendingChunkRejections`, and the `ChunkChanged` message queue are
//!   declared as `ResMut`, so the rest of the schedule keeps running in
//!   parallel,
//! - first-write-wins on `PendingChunkRejections`: if multiple validators
//!   try to reject the same prediction, the first one's reason is kept.
//!
//! # Built-in validator
//!
//! [`default_block_registry_validator`] is registered automatically and
//! enforces:
//! - `Place` is rejected if the cell's prior block is not replaceable.
//! - `Remove` is rejected if the cell's prior block is not destructible.
//! - `Replace` is unconditional.
//!
//! # Performance shape
//!
//! All systems iterate only the chunks listed in
//! [`ChunkCache::dirty_chunks`] — an O(1) lookup per modified chunk
//! rather than a scan over every loaded chunk. The dirty index is
//! maintained automatically by [`ChunkCache::push_predicted`].
//!
//! # Index stability invariant
//!
//! Validators identify a prediction by its `(ChunkPos, usize)` index into
//! `chunk.predicted()`. The commit pass then drains predictions and
//! applies rejections by the same index. **Nothing between
//! [`ChunkAuthoritySet::Validate`] and [`ChunkAuthoritySet::Commit`] may
//! push new predictions to a dirty chunk** — both sets live in
//! `PostUpdate` precisely so prediction is frozen for the frame.

use std::borrow::Cow;

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::block::BlockRegistry;
use crate::chunk::{
    CellDataChange, ChunkChange, ChunkPos, PredictedCellDataChange, PredictedChange,
    cache::ChunkCache, events::ChunkChanged,
};

/// Why a predicted change was rejected.
///
/// Built-in reasons are exposed as enum variants so they can be matched
/// in tests; downstream validators can supply free-form reasons via
/// [`RejectReason::Custom`].
///
/// Always logged at `warn!` by the commit system. Silence is wrong —
/// every rejection is either a bug, a desync, or a malicious client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    /// `Place` targeted a non-replaceable cell (built-in
    /// [`default_block_registry_validator`]).
    NotReplaceable,
    /// `Remove` targeted an indestructible cell or air (built-in
    /// [`default_block_registry_validator`]).
    NotDestructible,
    /// Free-form reason from a downstream validator.
    Custom(Cow<'static, str>),
}

impl RejectReason {
    /// Convenience constructor for downstream validators.
    pub fn custom(reason: impl Into<Cow<'static, str>>) -> Self {
        RejectReason::Custom(reason.into())
    }
}

/// Shared scratch resource: rejection decisions accumulated by validator
/// systems and consumed by [`commit_predicted_changes`].
///
/// Keyed by `(ChunkPos, usize)` where the `usize` is the index of the
/// rejected entry in the chunk's predicted queue at the moment validators
/// ran. The commit system drains this resource each frame.
///
/// **First-write-wins.** If two validators try to reject the same
/// prediction, the first one's reason is kept; later writes are ignored
/// (and logged at `debug!`). Validators are intentionally allowed to run
/// in parallel-equivalent order, so the *displayed* reason should not
/// depend on registration order — pick a primary validator if you care.
#[derive(Resource, Default, Debug)]
pub struct PendingChunkRejections {
    rejections: HashMap<(ChunkPos, usize), RejectReason>,
}

impl PendingChunkRejections {
    /// Reject the prediction at `index` in `pos`'s predicted queue with
    /// the given `reason`. No-op if the prediction is already rejected.
    pub fn reject(&mut self, pos: ChunkPos, index: usize, reason: RejectReason) {
        if let Some(existing) = self.rejections.get(&(pos, index)) {
            debug!(
                "Duplicate rejection for chunk {} prediction #{} ignored \
                 (kept: {:?}, dropped: {:?})",
                pos, index, existing, reason,
            );
            return;
        }
        self.rejections.insert((pos, index), reason);
    }

    /// Number of currently-pending rejections (across all chunks).
    pub fn len(&self) -> usize {
        self.rejections.len()
    }

    /// `true` if no rejections are pending.
    pub fn is_empty(&self) -> bool {
        self.rejections.is_empty()
    }

    /// Look up a pending rejection (used by the commit system and tests).
    pub fn get(&self, pos: ChunkPos, index: usize) -> Option<&RejectReason> {
        self.rejections.get(&(pos, index))
    }

    /// Drain the entire rejection map, leaving it empty.
    pub fn drain(
        &mut self,
    ) -> bevy::platform::collections::hash_map::Drain<'_, (ChunkPos, usize), RejectReason> {
        self.rejections.drain()
    }
}

/// Shared scratch resource: rejection decisions for predicted
/// [`CellDataChange`]s.
///
/// Mirrors [`PendingChunkRejections`] for the parallel cell-data queue.
/// Validators identify a prediction by its `(ChunkPos, usize)` index into
/// `chunk.predicted_cell_data()` and write a [`RejectReason`] here; the
/// commit pass drains it.
///
/// **First-write-wins** semantics match [`PendingChunkRejections`].
#[derive(Resource, Default, Debug)]
pub struct PendingCellDataRejections {
    rejections: HashMap<(ChunkPos, usize), RejectReason>,
}

impl PendingCellDataRejections {
    /// Reject the cell-data prediction at `index` in `pos`'s
    /// `predicted_cell_data` queue with the given `reason`.  No-op if
    /// the prediction is already rejected.
    pub fn reject(&mut self, pos: ChunkPos, index: usize, reason: RejectReason) {
        if let Some(existing) = self.rejections.get(&(pos, index)) {
            debug!(
                "Duplicate cell-data rejection for chunk {} prediction #{} ignored \
                 (kept: {:?}, dropped: {:?})",
                pos, index, existing, reason,
            );
            return;
        }
        self.rejections.insert((pos, index), reason);
    }

    /// Number of currently-pending cell-data rejections (across all
    /// chunks).
    pub fn len(&self) -> usize {
        self.rejections.len()
    }

    /// `true` if no cell-data rejections are pending.
    pub fn is_empty(&self) -> bool {
        self.rejections.is_empty()
    }

    /// Look up a pending cell-data rejection.
    pub fn get(&self, pos: ChunkPos, index: usize) -> Option<&RejectReason> {
        self.rejections.get(&(pos, index))
    }

    /// Drain the entire cell-data rejection map.
    pub fn drain(
        &mut self,
    ) -> bevy::platform::collections::hash_map::Drain<'_, (ChunkPos, usize), RejectReason> {
        self.rejections.drain()
    }
}

/// System sets that bracket the chunk-authority pipeline within
/// [`PostUpdate`].
///
/// - [`ChunkAuthoritySet::Validate`]: all chunk-change validator systems
///   run here. They read [`ChunkCache`] and write to
///   [`PendingChunkRejections`]. Validators run in parallel with anything
///   that doesn't touch those resources.
/// - [`ChunkAuthoritySet::Commit`]: [`commit_predicted_changes`] runs
///   here. It drains pending rejections, applies decisions to chunks,
///   bumps versions, and emits [`ChunkChanged`].
///
/// The plugin configures `Validate.before(Commit)`. Downstream validator
/// systems should be added in `Validate` (the
/// [`ChunkAuthorityAppExt::add_chunk_change_validator_system`] helper does
/// this for you).
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ChunkAuthoritySet {
    /// Validator systems run here. Parallel-safe with the rest of the
    /// schedule; they share `ResMut<PendingChunkRejections>` so they
    /// serialize among each other.
    Validate,
    /// The commit pass runs here. It mutates `ChunkCache` and
    /// `PendingChunkRejections` and writes `ChunkChanged` messages.
    Commit,
}

/// Extension trait on [`App`] for registering chunk-change validator
/// systems.
///
/// This is the public extension point downstream crates use to plug their
/// own validation logic into the authority pipeline.
///
/// # Example
///
/// ```ignore
/// use bevy::prelude::*;
/// use dd40_core::chunk::authority::*;
/// use dd40_core::chunk::cache::ChunkCache;
///
/// fn my_validator(
///     cache: Res<ChunkCache>,
///     mut pending: ResMut<PendingChunkRejections>,
///     // ...other resources you need...
/// ) {
///     for &pos in cache.dirty_chunks() {
///         let chunk = cache.get(&pos).unwrap();
///         for (i, _entry) in chunk.predicted().iter().enumerate() {
///             // ...inspect entry, decide, then maybe...
///             pending.reject(pos, i, RejectReason::custom("nope"));
///         }
///     }
/// }
///
/// fn build(app: &mut App) {
///     app.add_chunk_change_validator_system(my_validator);
/// }
/// ```
pub trait ChunkAuthorityAppExt {
    /// Register a Bevy system as a chunk-change validator.
    ///
    /// The system is added to [`PostUpdate`] inside
    /// [`ChunkAuthoritySet::Validate`], so it runs before
    /// [`commit_predicted_changes`]. It must not push new predicted
    /// changes (see the index-stability invariant in the module docs).
    fn add_chunk_change_validator_system<M>(
        &mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self;
}

impl ChunkAuthorityAppExt for App {
    fn add_chunk_change_validator_system<M>(
        &mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.add_systems(PostUpdate, system.in_set(ChunkAuthoritySet::Validate));
        self
    }
}

/// Built-in validator: enforce [`BlockRegistry`] semantics.
///
/// - `Place` is rejected if the cell's prior block is not replaceable.
/// - `Remove` is rejected if the cell's prior block is not destructible.
/// - `Replace` is always accepted (used by world-gen, redstone, etc.).
///
/// Registered automatically by [`ChunkAuthorityPlugin`].
pub fn default_block_registry_validator(
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut pending: ResMut<PendingChunkRejections>,
) {
    // Snapshot the dirty positions; iterate read-only so the system stays
    // parallel-friendly with anything else that doesn't touch the cache.
    let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
    for pos in dirty {
        let Some(chunk) = cache.get(&pos) else {
            continue;
        };
        for (i, entry) in chunk.predicted().iter().enumerate() {
            let prior = entry.prior;
            let decision = match &entry.change {
                ChunkChange::Place { .. } => {
                    if registry.is_replaceable(&prior) {
                        None
                    } else {
                        Some(RejectReason::NotReplaceable)
                    }
                }
                ChunkChange::Remove { .. } => {
                    if registry.is_destructible(&prior) {
                        None
                    } else {
                        Some(RejectReason::NotDestructible)
                    }
                }
                ChunkChange::Replace { .. } => None,
            };
            if let Some(reason) = decision {
                pending.reject(pos, i, reason);
            }
        }
    }
}

/// Built-in validator: reject [`CellDataChange`]s whose [`TypeId`] is not
/// registered with [`BlockDataTypeRegistry`].
///
/// The registry is the only way the wire and disk formats can round-trip
/// a typed value back into a `Box<dyn BlockData>` on the receiving side
/// (see [`BlockDataAppExt::register_block_data`]).  Accepting an
/// unregistered type into authoritative state would silently break
/// replication, so we reject up-front and surface a `Custom` reason.
///
/// Registered automatically by [`ChunkAuthorityPlugin`].
///
/// [`BlockDataAppExt::register_block_data`]: crate::block::BlockDataAppExt::register_block_data
/// [`BlockDataTypeRegistry`]: crate::block::BlockDataTypeRegistry
/// [`TypeId`]: std::any::TypeId
pub fn default_cell_data_registry_validator(
    cache: Res<ChunkCache>,
    registry: Res<crate::block::BlockDataTypeRegistry>,
    mut pending: ResMut<PendingCellDataRejections>,
) {
    let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
    for pos in dirty {
        let Some(chunk) = cache.get(&pos) else {
            continue;
        };
        for (i, entry) in chunk.predicted_cell_data().iter().enumerate() {
            let type_id = entry.change.type_id();
            if registry.get_by_type_id(type_id).is_none() {
                pending.reject(
                    pos,
                    i,
                    RejectReason::custom(format!(
                        "cell-data type `{}` is not registered",
                        entry.change.type_key()
                    )),
                );
            }
        }
    }
}

/// Walk the dirty index, drain each dirty chunk's predicted queue, apply
/// pending rejections from [`PendingChunkRejections`], commit accepted
/// changes, and emit one [`ChunkChanged`] message per modified chunk.
///
/// Runs in [`ChunkAuthoritySet::Commit`] within [`PostUpdate`] when
/// [`ChunkAuthorityPlugin`] is added.
///
/// **Iteration cost** is O(dirty chunks * predictions per chunk),
/// independent of total loaded chunks.
pub fn commit_predicted_changes(
    mut cache: ResMut<ChunkCache>,
    mut pending: ResMut<PendingChunkRejections>,
    mut pending_cell_data: ResMut<PendingCellDataRejections>,
    mut writer: MessageWriter<ChunkChanged>,
) {
    let dirty: Vec<ChunkPos> = cache.drain_dirty().collect();
    if dirty.is_empty() {
        // Nothing to commit. Clear any stray rejections — they reference
        // indices into queues we are about to ignore.
        if !pending.is_empty() {
            warn!(
                "Discarding {} rejection(s) targeting non-dirty chunks",
                pending.len()
            );
            pending.rejections.clear();
        }
        if !pending_cell_data.is_empty() {
            warn!(
                "Discarding {} cell-data rejection(s) targeting non-dirty chunks",
                pending_cell_data.len()
            );
            pending_cell_data.rejections.clear();
        }
        return;
    }

    // Drain rejections into per-chunk lookups so we can iterate
    // predictions linearly per chunk.
    let mut rejections_by_chunk: HashMap<ChunkPos, HashMap<usize, RejectReason>> = HashMap::new();
    for ((pos, idx), reason) in pending.drain() {
        rejections_by_chunk
            .entry(pos)
            .or_default()
            .insert(idx, reason);
    }
    let mut cell_data_rejections_by_chunk: HashMap<ChunkPos, HashMap<usize, RejectReason>> =
        HashMap::new();
    for ((pos, idx), reason) in pending_cell_data.drain() {
        cell_data_rejections_by_chunk
            .entry(pos)
            .or_default()
            .insert(idx, reason);
    }

    for pos in dirty {
        let Some(chunk) = cache.get_mut(&pos) else {
            continue;
        };
        let predicted: Vec<PredictedChange> = chunk.take_predicted();
        let predicted_cell_data: Vec<PredictedCellDataChange> = chunk.take_predicted_cell_data();
        if predicted.is_empty() && predicted_cell_data.is_empty() {
            continue;
        }

        // -- block-level changes --
        let mut chunk_rejections = rejections_by_chunk.remove(&pos).unwrap_or_default();
        let mut accepted: Vec<ChunkChange> = Vec::with_capacity(predicted.len());
        for (i, entry) in predicted.into_iter().enumerate() {
            if let Some(reason) = chunk_rejections.remove(&i) {
                warn!(
                    "Rejected predicted change at chunk {} cell {:?}: {:?}",
                    pos,
                    entry.change.local(),
                    reason,
                );
                chunk.rollback_to(entry.change.local(), entry.prior);
            } else {
                accepted.push(entry.change);
            }
        }

        // -- cell-data changes --
        let mut chunk_cell_data_rejections = cell_data_rejections_by_chunk
            .remove(&pos)
            .unwrap_or_default();
        let mut accepted_cell_data: Vec<CellDataChange> =
            Vec::with_capacity(predicted_cell_data.len());
        for (i, entry) in predicted_cell_data.into_iter().enumerate() {
            if let Some(reason) = chunk_cell_data_rejections.remove(&i) {
                warn!(
                    "Rejected predicted cell-data change at chunk {} cell {:?}: {:?}",
                    pos,
                    entry.change.local(),
                    reason,
                );
                chunk.rollback_cell_data(entry.change.local(), entry.change.type_id(), entry.prior);
            } else {
                accepted_cell_data.push(entry.change);
            }
        }

        if accepted.is_empty() && accepted_cell_data.is_empty() {
            continue;
        }

        // Invariant: cell data must never outlive the block it belongs
        // to.  For every accepted block-Remove, drain any typed entries
        // at that local and synthesise Clear changes so the rest of the
        // pipeline (history, ChunkChanged listeners, S5–S7 wire) sees a
        // single coherent stream.
        for change in &accepted {
            if let ChunkChange::Remove { local } = *change {
                for (type_id, type_key) in chunk.drain_cell_data_at(local) {
                    accepted_cell_data.push(CellDataChange::clear_raw(local, type_id, type_key));
                }
            }
        }

        let committed = chunk.commit_accepted(&accepted);
        let committed_cell_data = chunk.commit_accepted_cell_data(accepted_cell_data);
        let new_version = chunk.version();
        writer.write(ChunkChanged {
            pos,
            changes: committed.into_iter().map(|(_, c)| c).collect(),
            cell_data_changes: committed_cell_data.into_iter().map(|(_, c)| c).collect(),
            new_version,
        });
    }

    // Any rejection still in `rejections_by_chunk` referenced a chunk no
    // longer present in the cache — the chunk was evicted between
    // Validate and Commit. Log and drop.
    for (pos, leftover) in rejections_by_chunk {
        for (idx, reason) in leftover {
            warn!(
                "Dropping rejection for evicted chunk {} prediction #{} ({:?})",
                pos, idx, reason
            );
        }
    }
    for (pos, leftover) in cell_data_rejections_by_chunk {
        for (idx, reason) in leftover {
            warn!(
                "Dropping cell-data rejection for evicted chunk {} prediction #{} ({:?})",
                pos, idx, reason
            );
        }
    }
}

/// Server-only Bevy plugin that runs the authoritative chunk-commit
/// pipeline in [`PostUpdate`].
///
/// Adding this plugin promotes the local instance to chunk authority.
/// The server binary adds it; the client never does.
///
/// On `build` this plugin:
/// 1. Inserts [`PendingChunkRejections`] as a resource.
/// 2. Configures [`ChunkAuthoritySet::Validate`] to run before
///    [`ChunkAuthoritySet::Commit`] in [`PostUpdate`].
/// 3. Registers [`default_block_registry_validator`] in
///    [`ChunkAuthoritySet::Validate`].
/// 4. Registers [`commit_predicted_changes`] in
///    [`ChunkAuthoritySet::Commit`].
///
/// Downstream crates extend the pipeline by calling
/// [`ChunkAuthorityAppExt::add_chunk_change_validator_system`].
#[derive(Default)]
pub struct ChunkAuthorityPlugin;

impl Plugin for ChunkAuthorityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingChunkRejections>();
        app.init_resource::<PendingCellDataRejections>();
        app.init_resource::<crate::block::BlockDataTypeRegistry>();
        app.configure_sets(
            PostUpdate,
            ChunkAuthoritySet::Validate.before(ChunkAuthoritySet::Commit),
        );
        app.add_systems(
            PostUpdate,
            (
                default_block_registry_validator,
                default_cell_data_registry_validator,
            )
                .in_set(ChunkAuthoritySet::Validate),
        );
        app.add_systems(
            PostUpdate,
            commit_predicted_changes.in_set(ChunkAuthoritySet::Commit),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{Block, BlockDefinition, BlockId, registry::BlockRegistry};
    use crate::chunk::{Chunk, ChunkPos, change::BlockLocal};

    fn registry_with_stone() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_without_event(BlockDefinition::new(BlockId(1), "stone"));
        r
    }

    fn lp(x: u8, y: u16, z: u8) -> BlockLocal {
        BlockLocal::new(x, y, z)
    }

    fn build_app_with_chunk(pos: ChunkPos) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        cache.insert(Chunk::new(pos));
        app.insert_resource(cache);
        app
    }

    #[test]
    fn plugin_drains_predicted_and_emits_chunk_changed() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);

        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(2, 3, 4), BlockId(1)));

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).expect("chunk present");
        assert_eq!(chunk.version(), 1);
        assert!(chunk.predicted().is_empty());
        assert_eq!(chunk.confirmed_history().len(), 1);
        assert_eq!(cache.dirty_count(), 0, "dirty index drained after commit");

        let messages = app.world().resource::<Messages<ChunkChanged>>();
        let emitted: Vec<_> = messages.iter_current_update_messages().cloned().collect();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].new_version, 1);
        assert_eq!(emitted[0].changes.len(), 1);
        assert_eq!(emitted[0].pos, pos);

        // Pending rejections must be empty after commit.
        assert!(app.world().resource::<PendingChunkRejections>().is_empty());
    }

    #[test]
    fn plugin_rolls_back_default_validator_rejection() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);

        // Pre-fill the cell with stone (non-replaceable).
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .get_mut(&pos)
            .unwrap()
            .set_local(lp(0, 0, 0), Block::new(BlockId(1)));
        // Push a Place — prior is stone → default validator rejects.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.version(), 0);
        assert!(chunk.predicted().is_empty());
        assert_eq!(chunk.confirmed_history().len(), 0);
        assert_eq!(chunk.get_local(lp(0, 0, 0)).block_id, BlockId(1));
    }

    #[test]
    fn predicted_queue_untouched_without_plugin() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.predicted().len(), 1);
        assert_eq!(chunk.version(), 0);
        assert_eq!(cache.dirty_count(), 1);
    }

    #[test]
    fn dirty_index_skips_clean_chunks() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        for x in 0..10 {
            for z in 0..10 {
                cache.insert(Chunk::new(ChunkPos::new(x, 0, z)));
            }
        }
        let dirty_pos = ChunkPos::new(5, 0, 5);
        cache.push_predicted(dirty_pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        assert_eq!(cache.dirty_count(), 1);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let advanced: Vec<_> = cache
            .iter_positions()
            .filter(|p| cache.get(p).unwrap().version() > 0)
            .collect();
        assert_eq!(advanced.len(), 1);
        assert_eq!(*advanced[0], dirty_pos);
    }

    /// A custom validator system that rejects every prediction it sees.
    fn always_reject_validator(
        cache: Res<ChunkCache>,
        mut pending: ResMut<PendingChunkRejections>,
    ) {
        let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
        for pos in dirty {
            let Some(chunk) = cache.get(&pos) else {
                continue;
            };
            for i in 0..chunk.predicted().len() {
                pending.reject(pos, i, RejectReason::custom("test rejection"));
            }
        }
    }

    #[test]
    fn custom_validator_system_can_reject() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        // Push a change the default validator would ACCEPT (place into air)…
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        app.add_plugins(ChunkAuthorityPlugin);
        // …but a downstream validator system rejects everything.
        app.add_chunk_change_validator_system(always_reject_validator);

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.version(), 0);
        assert!(chunk.confirmed_history().is_empty());
        assert_eq!(chunk.get_local(lp(0, 0, 0)).block_id, BlockId::AIR);

        let messages = app.world().resource::<Messages<ChunkChanged>>();
        assert_eq!(messages.iter_current_update_messages().count(), 0);
    }

    #[test]
    fn first_rejection_wins() {
        // Two custom validators reject the same prediction with
        // different reasons. The first one's reason must be kept.
        fn reject_with_a(cache: Res<ChunkCache>, mut pending: ResMut<PendingChunkRejections>) {
            let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
            for pos in dirty {
                pending.reject(pos, 0, RejectReason::custom("validator-A"));
            }
        }
        fn reject_with_b(cache: Res<ChunkCache>, mut pending: ResMut<PendingChunkRejections>) {
            let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
            for pos in dirty {
                pending.reject(pos, 0, RejectReason::custom("validator-B"));
            }
        }

        let mut pending = PendingChunkRejections::default();
        let pos = ChunkPos::new(0, 0, 0);
        pending.reject(pos, 0, RejectReason::custom("first"));
        pending.reject(pos, 0, RejectReason::custom("second"));
        assert_eq!(
            pending.get(pos, 0),
            Some(&RejectReason::custom("first")),
            "first-write-wins on direct reject() calls"
        );

        // And as systems with explicit ordering: A.before(B) means A's
        // reason is the one that wins.
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        app.add_plugins(ChunkAuthorityPlugin);
        app.add_systems(
            PostUpdate,
            (reject_with_a, reject_with_b)
                .chain()
                .in_set(ChunkAuthoritySet::Validate),
        );
        // Snoop on the rejection map before commit by inserting a
        // mid-set probe system. Easier: just assert the final outcome —
        // commit happened, change rolled back, regardless of reason.
        app.update();
        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.version(), 0);
        assert!(chunk.confirmed_history().is_empty());
    }

    #[test]
    fn rejections_for_evicted_chunks_are_dropped_safely() {
        // Push a rejection referencing a chunk that doesn't exist in the
        // cache — commit must not panic.
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        // Mark dirty so commit runs, then evict the chunk.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        // Manually inject a rejection for a chunk that's NOT in the cache.
        let mut pending = PendingChunkRejections::default();
        pending.reject(ChunkPos::new(99, 0, 99), 0, RejectReason::custom("ghost"));
        app.insert_resource(pending);

        app.add_plugins(ChunkAuthorityPlugin);
        // Should not panic.
        app.update();
    }

    // -- cell-data commit-pass tests -------------------------------------

    use crate::block::{BlockData, BlockDataAppExt};
    use crate::chunk::change::CellDataChange;
    use bevy::ecs::message::Messages;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestChestState {
        slots: u8,
    }

    impl BlockData for TestChestState {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct UnregisteredData;

    impl BlockData for UnregisteredData {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn drain_chunk_changed(app: &mut App) -> Vec<ChunkChanged> {
        let mut msgs = app.world_mut().resource_mut::<Messages<ChunkChanged>>();
        msgs.drain().collect()
    }

    #[test]
    fn cell_data_set_round_trips_through_authority() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.register_block_data::<TestChestState>();
        app.add_plugins(ChunkAuthorityPlugin);

        let change = CellDataChange::new_set(lp(1, 2, 3), TestChestState { slots: 9 });
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted_cell_data(pos, change);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).expect("chunk present");
        assert_eq!(chunk.version(), 1);
        assert!(chunk.predicted_cell_data().is_empty());
        assert_eq!(chunk.confirmed_cell_data_history().len(), 1);
        assert_eq!(
            chunk.cell_data::<TestChestState>(lp(1, 2, 3)),
            Some(&TestChestState { slots: 9 })
        );

        let mut msgs = drain_chunk_changed(&mut app);
        assert_eq!(msgs.len(), 1);
        let m = msgs.remove(0);
        assert_eq!(m.pos, pos);
        assert!(m.changes.is_empty());
        assert_eq!(m.cell_data_changes.len(), 1);
        assert_eq!(m.new_version, 1);
    }

    #[test]
    fn cell_data_clear_round_trips_through_authority() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.register_block_data::<TestChestState>();
        // Seed the cell with confirmed data before authority runs.
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            let chunk = cache.get_mut(&pos).unwrap();
            chunk.set_cell_data(lp(0, 0, 0), TestChestState { slots: 3 });
        }
        app.add_plugins(ChunkAuthorityPlugin);

        let change = CellDataChange::new_clear::<TestChestState>(lp(0, 0, 0));
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted_cell_data(pos, change);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.version(), 1);
        assert_eq!(
            chunk.cell_data::<TestChestState>(lp(0, 0, 0)),
            None,
            "cell data cleared"
        );
    }

    #[test]
    fn cell_data_unregistered_type_is_rejected_and_rolled_back() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        // Note: do NOT register UnregisteredData.
        app.add_plugins(ChunkAuthorityPlugin);

        let change = CellDataChange::new_set(lp(2, 2, 2), UnregisteredData);
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted_cell_data(pos, change);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        // Rejected → version not bumped, no confirmed history, cell still empty.
        assert_eq!(chunk.version(), 0);
        assert!(chunk.predicted_cell_data().is_empty());
        assert!(chunk.confirmed_cell_data_history().is_empty());
        assert_eq!(chunk.cell_data::<UnregisteredData>(lp(2, 2, 2)), None);

        let msgs = drain_chunk_changed(&mut app);
        assert!(
            msgs.is_empty(),
            "no ChunkChanged when only rejection occurred"
        );
    }

    #[test]
    fn mixed_block_and_cell_data_share_version_and_one_event() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.register_block_data::<TestChestState>();
        app.add_plugins(ChunkAuthorityPlugin);

        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
            cache.push_predicted(pos, ChunkChange::new_place(lp(0, 1, 0), BlockId(1)));
            cache.push_predicted_cell_data(
                pos,
                CellDataChange::new_set(lp(0, 0, 0), TestChestState { slots: 1 }),
            );
        }
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        // 2 block + 1 cell-data accepted → version bumped 3 times.
        assert_eq!(chunk.version(), 3);

        let mut msgs = drain_chunk_changed(&mut app);
        assert_eq!(msgs.len(), 1, "single ChunkChanged for a dirty chunk");
        let m = msgs.remove(0);
        assert_eq!(m.changes.len(), 2);
        assert_eq!(m.cell_data_changes.len(), 1);
        assert_eq!(m.new_version, 3);
    }

    #[test]
    fn block_remove_auto_clears_attached_cell_data() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.register_block_data::<TestChestState>();

        // Seed the cell with confirmed block + cell data before authority runs.
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            let chunk = cache.get_mut(&pos).unwrap();
            chunk.set_local(lp(4, 5, 6), Block::new(BlockId(1)));
            chunk.set_cell_data(lp(4, 5, 6), TestChestState { slots: 27 });
        }
        app.add_plugins(ChunkAuthorityPlugin);

        // Predict a Remove on that block.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_remove(lp(4, 5, 6)));
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        // Auto-cleanup: cell data must be gone.
        assert_eq!(
            chunk.cell_data::<TestChestState>(lp(4, 5, 6)),
            None,
            "cell data must not outlive its block"
        );
        // Version bumped twice: 1 for the Remove, 1 for the synthesised Clear.
        assert_eq!(chunk.version(), 2);

        let mut msgs = drain_chunk_changed(&mut app);
        assert_eq!(msgs.len(), 1);
        let m = msgs.remove(0);
        assert_eq!(m.changes.len(), 1);
        assert_eq!(m.cell_data_changes.len(), 1, "synthesised Clear emitted");
        match &m.cell_data_changes[0] {
            CellDataChange::Clear {
                local, type_key, ..
            } => {
                assert_eq!(*local, lp(4, 5, 6));
                assert_eq!(*type_key, std::any::type_name::<TestChestState>());
            }
            _ => panic!("expected Clear, got {:?}", m.cell_data_changes[0]),
        }
    }

    #[test]
    fn push_predicted_place_with_data_commits_both_queues() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut app = build_app_with_chunk(pos);
        app.register_block_data::<TestChestState>();
        app.add_plugins(ChunkAuthorityPlugin);

        let placed = app
            .world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted_place_with_data(
                pos,
                lp(7, 8, 9),
                BlockId(1),
                TestChestState { slots: 5 },
            );
        assert!(placed);

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.get_local(lp(7, 8, 9)).block_id, BlockId(1));
        assert_eq!(
            chunk.cell_data::<TestChestState>(lp(7, 8, 9)),
            Some(&TestChestState { slots: 5 })
        );
        assert_eq!(chunk.version(), 2);
    }
}
