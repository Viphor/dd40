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
//! # Performance shape
//!
//! The commit pass iterates only the chunks listed in
//! [`ChunkCache::dirty_chunks`] — an O(1) lookup per modified chunk
//! rather than a scan over every loaded chunk. The dirty index is
//! maintained automatically by [`ChunkCache::push_predicted`].
//!
//! Validators receive `&World` (read-only), not `&mut World`, so they
//! can freely access shared state without escalating the system to
//! exclusive-write contention beyond what the commit pass itself
//! requires.
//!
//! Cross-chunk parallelism is a future optimisation — the dirty list is
//! the much bigger win, and validators are pure-read which makes a
//! `bevy::tasks::ComputeTaskPool` rollout straightforward when needed.
//!
//! # Extending the validator chain
//!
//! Acceptance/rejection of each predicted change is decided by an
//! ordered chain of registered [`ChunkChangeValidator`]s, NOT by a
//! hard-coded match. This is by design — see the "Flexibility Over
//! Convenience" section of `CLAUDE.md` and `.github/copilot-instructions.md`.
//! Built-in validators ship from `dd40_core` (currently
//! [`DefaultBlockRegistryValidator`]); downstream crates that own
//! resources the core can't see (e.g. `CharacterSpatialCache`) register
//! their own validators via [`ChunkAuthorityAppExt::add_chunk_change_validator`].
//!
//! Validators run in registration order. The first validator that returns
//! [`CommitDecision::Reject`] short-circuits the chain — its reason is
//! logged and the change is rolled back. If every validator returns
//! [`CommitDecision::Accept`], the change is committed.

use std::borrow::Cow;

use bevy::ecs::message::Messages;
use bevy::prelude::*;

use crate::block::{Block, BlockRegistry};
use crate::chunk::{
    ChunkChange, ChunkPos, PredictedChange,
    cache::ChunkCache,
    events::ChunkChanged,
};

/// Outcome of validating one predicted change against its chunk's
/// pre-prediction state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitDecision {
    /// This validator approves the change. The next validator in the
    /// chain runs; if no validator rejects, the change is committed.
    Accept,
    /// This validator rejects the change. The chain short-circuits and
    /// the reason is logged at `warn!`.
    Reject(RejectReason),
}

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
    /// [`DefaultBlockRegistryValidator`]).
    NotReplaceable,
    /// `Remove` targeted an indestructible cell or air (built-in
    /// [`DefaultBlockRegistryValidator`]).
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

/// A pluggable validator that decides whether one predicted
/// [`ChunkChange`] should be committed.
///
/// Validators take an immutable `&World` reference, allowing them to
/// read any resource or run their own [`World::iter_entities`]-style
/// queries without forcing the commit system to take a unique borrow on
/// each one. Downstream validators that genuinely require cached query
/// state can wrap a `SystemState` in a [`std::sync::Mutex`] and update
/// it via interior mutability.
///
/// Validators are `&self` — pure with respect to their own state — so
/// future cross-chunk parallelism stays trivial to implement.
///
/// # Implementing
///
/// ```ignore
/// use dd40_core::chunk::authority::*;
/// use dd40_core::block::Block;
/// use dd40_core::chunk::{ChunkChange, ChunkPos};
/// use bevy::prelude::*;
///
/// struct MyValidator;
///
/// impl ChunkChangeValidator for MyValidator {
///     fn validate(
///         &self,
///         _world: &World,
///         _change: &ChunkChange,
///         _prior: Block,
///         _chunk_pos: ChunkPos,
///     ) -> CommitDecision {
///         CommitDecision::Accept
///     }
/// }
/// ```
pub trait ChunkChangeValidator: Send + Sync + 'static {
    /// Decide whether `change` is acceptable.
    ///
    /// `prior` is the block that occupied the target cell **before** the
    /// change was optimistically applied — this is what the validator
    /// should reason about, not the current value in `data`.
    fn validate(
        &self,
        world: &World,
        change: &ChunkChange,
        prior: Block,
        chunk_pos: ChunkPos,
    ) -> CommitDecision;
}

/// Registered chain of [`ChunkChangeValidator`]s consulted by the commit
/// pass.
///
/// Inserted automatically by [`ChunkAuthorityPlugin`] and seeded with
/// [`DefaultBlockRegistryValidator`]. Downstream crates push additional
/// validators via [`ChunkAuthorityAppExt::add_chunk_change_validator`].
#[derive(Resource, Default)]
pub struct ChunkChangeValidators {
    validators: Vec<Box<dyn ChunkChangeValidator>>,
}

impl ChunkChangeValidators {
    /// Append a validator to the end of the chain.
    pub fn push<V: ChunkChangeValidator>(&mut self, validator: V) {
        self.validators.push(Box::new(validator));
    }

    /// Number of registered validators.
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// `true` if no validators are registered.
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

/// Extension trait on [`App`] for registering chunk-change validators.
///
/// This is the public extension point downstream crates use to plug
/// their own validation logic into the authority commit pass.
pub trait ChunkAuthorityAppExt {
    /// Register a [`ChunkChangeValidator`] on the validator chain.
    ///
    /// Validators run in registration order. The first validator that
    /// returns [`CommitDecision::Reject`] wins; otherwise the change is
    /// accepted.
    ///
    /// Inserts [`ChunkChangeValidators`] as a resource if it is not
    /// already present, so this can be called regardless of whether
    /// [`ChunkAuthorityPlugin`] has been added yet.
    fn add_chunk_change_validator<V: ChunkChangeValidator>(&mut self, validator: V) -> &mut Self;
}

impl ChunkAuthorityAppExt for App {
    fn add_chunk_change_validator<V: ChunkChangeValidator>(&mut self, validator: V) -> &mut Self {
        let world = self.world_mut();
        if !world.contains_resource::<ChunkChangeValidators>() {
            world.insert_resource(ChunkChangeValidators::default());
        }
        world
            .resource_mut::<ChunkChangeValidators>()
            .push(validator);
        self
    }
}

/// The built-in validator that enforces [`BlockRegistry`] semantics.
///
/// Registered automatically by [`ChunkAuthorityPlugin`]:
/// - `Place` rejects if the cell's prior block is not replaceable.
/// - `Remove` rejects if the cell's prior block is not destructible
///   (i.e. air or an indestructible block).
/// - `Replace` is unconditional (used by world generation, redstone, …).
pub struct DefaultBlockRegistryValidator;

impl ChunkChangeValidator for DefaultBlockRegistryValidator {
    fn validate(
        &self,
        world: &World,
        change: &ChunkChange,
        prior: Block,
        _chunk_pos: ChunkPos,
    ) -> CommitDecision {
        let registry = world.resource::<BlockRegistry>();
        match change {
            ChunkChange::Place { .. } => {
                if registry.is_replaceable(&prior) {
                    CommitDecision::Accept
                } else {
                    CommitDecision::Reject(RejectReason::NotReplaceable)
                }
            }
            ChunkChange::Remove { .. } => {
                if registry.is_destructible(&prior) {
                    CommitDecision::Accept
                } else {
                    CommitDecision::Reject(RejectReason::NotDestructible)
                }
            }
            ChunkChange::Replace { .. } => CommitDecision::Accept,
        }
    }
}

/// Run the registered validator chain against a single change.
///
/// Returns the first [`CommitDecision::Reject`] produced by a validator;
/// otherwise returns [`CommitDecision::Accept`].
fn run_validators(
    world: &World,
    validators: &ChunkChangeValidators,
    change: &ChunkChange,
    prior: Block,
    chunk_pos: ChunkPos,
) -> CommitDecision {
    for v in &validators.validators {
        let decision = v.validate(world, change, prior, chunk_pos);
        if matches!(decision, CommitDecision::Reject(_)) {
            return decision;
        }
    }
    CommitDecision::Accept
}

/// Exclusive system: walk the dirty index, drain each dirty chunk's
/// predicted queue, run it through the registered validator chain,
/// apply accepted changes, roll back rejected ones, and emit
/// [`ChunkChanged`] messages.
///
/// Runs in [`PostUpdate`] when [`ChunkAuthorityPlugin`] is added.
///
/// **Iteration cost** is O(dirty chunks * predictions per chunk *
/// validators), not O(loaded chunks). The dirty index is maintained by
/// [`ChunkCache::push_predicted`].
pub fn commit_predicted_changes(world: &mut World) {
    // Snapshot dirty positions and drain each dirty chunk's predicted
    // queue in one pass while we hold the cache lock.
    let work: Vec<(ChunkPos, Vec<PredictedChange>)> = {
        let mut cache = world.resource_mut::<ChunkCache>();
        let dirty: Vec<ChunkPos> = cache.drain_dirty().collect();
        dirty
            .into_iter()
            .filter_map(|pos| {
                let chunk = cache.get_mut(&pos)?;
                let predicted = chunk.take_predicted();
                if predicted.is_empty() {
                    None
                } else {
                    Some((pos, predicted))
                }
            })
            .collect()
    };

    if work.is_empty() {
        return;
    }

    // Run validators with resource_scope so we hold a unique borrow on
    // ChunkChangeValidators while still passing &World to each validator.
    // No remove/reinsert dance.
    let decisions: Vec<(ChunkPos, Vec<(PredictedChange, CommitDecision)>)> = world
        .resource_scope(|world, validators: Mut<ChunkChangeValidators>| {
            work.into_iter()
                .map(|(pos, predicted)| {
                    let chunk_decisions = predicted
                        .into_iter()
                        .map(|entry| {
                            let decision = run_validators(
                                world,
                                &validators,
                                &entry.change,
                                entry.prior,
                                pos,
                            );
                            (entry, decision)
                        })
                        .collect();
                    (pos, chunk_decisions)
                })
                .collect()
        });

    // Apply decisions: rollback rejected, commit accepted, build outgoing
    // ChunkChanged messages.
    let mut emit: Vec<ChunkChanged> = Vec::with_capacity(decisions.len());
    {
        let mut cache = world.resource_mut::<ChunkCache>();
        for (pos, chunk_decisions) in decisions {
            let Some(chunk) = cache.get_mut(&pos) else {
                continue;
            };
            let mut accepted: Vec<ChunkChange> = Vec::with_capacity(chunk_decisions.len());
            for (entry, decision) in chunk_decisions {
                match decision {
                    CommitDecision::Accept => accepted.push(entry.change),
                    CommitDecision::Reject(reason) => {
                        warn!(
                            "Rejected predicted change at chunk {} cell {:?}: {:?}",
                            pos,
                            entry.change.local(),
                            reason,
                        );
                        chunk.rollback_to(entry.change.local(), entry.prior);
                    }
                }
            }
            if accepted.is_empty() {
                continue;
            }
            let committed = chunk.commit_accepted(&accepted);
            let new_version = chunk.version();
            emit.push(ChunkChanged {
                pos,
                changes: committed.into_iter().map(|(_, c)| c).collect(),
                new_version,
            });
        }
    }

    let mut writer = world.resource_mut::<Messages<ChunkChanged>>();
    for msg in emit {
        writer.write(msg);
    }
}

/// Server-only Bevy plugin that adds the authoritative
/// [`commit_predicted_changes`] system in [`PostUpdate`] and seeds the
/// validator chain with [`DefaultBlockRegistryValidator`].
///
/// Adding this plugin promotes the local instance to chunk authority.
/// The server binary adds it; the client never does.
#[derive(Default)]
pub struct ChunkAuthorityPlugin;

impl Plugin for ChunkAuthorityPlugin {
    fn build(&self, app: &mut App) {
        app.add_chunk_change_validator(DefaultBlockRegistryValidator);
        app.add_systems(PostUpdate, commit_predicted_changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockDefinition, BlockId, registry::BlockRegistry};
    use crate::chunk::{Chunk, ChunkPos, change::BlockLocal};

    fn registry_with_stone() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_without_event(BlockDefinition::new(BlockId(1), "stone"));
        r
    }

    fn lp(x: u8, y: u16, z: u8) -> BlockLocal {
        BlockLocal::new(x, y, z)
    }

    /// Helper: build a throwaway World with a registry and run the
    /// default validator against one change.
    fn run_default(change: &ChunkChange, prior: Block, registry: BlockRegistry) -> CommitDecision {
        let mut world = World::new();
        world.insert_resource(registry);
        let v = DefaultBlockRegistryValidator;
        v.validate(&world, change, prior, ChunkPos::new(0, 0))
    }

    #[test]
    fn default_accepts_place_into_air() {
        let c = ChunkChange::new_place(lp(0, 0, 0), BlockId(1));
        assert_eq!(
            run_default(&c, Block::default(), registry_with_stone()),
            CommitDecision::Accept,
        );
    }

    #[test]
    fn default_rejects_place_into_solid() {
        let c = ChunkChange::new_place(lp(0, 0, 0), BlockId(1));
        assert_eq!(
            run_default(&c, Block::new(BlockId(1)), registry_with_stone()),
            CommitDecision::Reject(RejectReason::NotReplaceable),
        );
    }

    #[test]
    fn default_accepts_remove_of_destructible() {
        let c = ChunkChange::new_remove(lp(0, 0, 0));
        assert_eq!(
            run_default(&c, Block::new(BlockId(1)), registry_with_stone()),
            CommitDecision::Accept,
        );
    }

    #[test]
    fn default_rejects_remove_of_air() {
        let c = ChunkChange::new_remove(lp(0, 0, 0));
        assert_eq!(
            run_default(&c, Block::default(), registry_with_stone()),
            CommitDecision::Reject(RejectReason::NotDestructible),
        );
    }

    #[test]
    fn default_accepts_replace_unconditionally() {
        let c = ChunkChange::new_replace(lp(0, 0, 0), BlockId(1));
        assert_eq!(
            run_default(&c, Block::new(BlockId(1)), registry_with_stone()),
            CommitDecision::Accept,
        );
    }

    /// Build a Bevy app pre-populated with a chunk at `pos` containing
    /// no predictions, and seeds the registry resource.
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
        let pos = ChunkPos::new(0, 0);
        let mut app = build_app_with_chunk(pos);

        // Use the canonical entry point — also marks the chunk dirty.
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
    }

    #[test]
    fn plugin_rolls_back_rejected_prediction() {
        let pos = ChunkPos::new(0, 0);
        let mut app = build_app_with_chunk(pos);

        // Pre-fill the cell with stone (non-replaceable). This is the
        // pre-prediction state.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .get_mut(&pos)
            .unwrap()
            .set_local(lp(0, 0, 0), Block::new(BlockId(1)));
        // Push a Place — the prior captured by push_predicted is stone,
        // so validation rejects the change.
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
        let pos = ChunkPos::new(0, 0);
        let mut app = build_app_with_chunk(pos);
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        // No ChunkAuthorityPlugin — predicted queue must survive.
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&pos).unwrap();
        assert_eq!(chunk.predicted().len(), 1);
        assert_eq!(chunk.version(), 0);
        // The dirty index also persists — caller (e.g. a future client
        // ChunkUpdate handler) is expected to drain it.
        assert_eq!(cache.dirty_count(), 1);
    }

    #[test]
    fn dirty_index_skips_clean_chunks() {
        // 100 chunks, only one with a prediction. The commit pass must
        // touch exactly one chunk.
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        for x in 0..10 {
            for z in 0..10 {
                cache.insert(Chunk::new(ChunkPos::new(x, z)));
            }
        }
        let dirty_pos = ChunkPos::new(5, 5);
        cache.push_predicted(dirty_pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        assert_eq!(cache.dirty_count(), 1);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        // Exactly one chunk advanced past version 0.
        let cache = app.world().resource::<ChunkCache>();
        let advanced: Vec<_> = cache
            .iter_positions()
            .filter(|p| cache.get(p).unwrap().version() > 0)
            .collect();
        assert_eq!(advanced.len(), 1);
        assert_eq!(*advanced[0], dirty_pos);
    }

    /// A custom validator that rejects every change with a custom reason.
    struct AlwaysRejectValidator;

    impl ChunkChangeValidator for AlwaysRejectValidator {
        fn validate(
            &self,
            _world: &World,
            _change: &ChunkChange,
            _prior: Block,
            _chunk_pos: ChunkPos,
        ) -> CommitDecision {
            CommitDecision::Reject(RejectReason::custom("test rejection"))
        }
    }

    #[test]
    fn registered_custom_validator_is_consulted() {
        let pos = ChunkPos::new(0, 0);
        let mut app = build_app_with_chunk(pos);
        // Push a change the default validator would ACCEPT (place into air)…
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        app.add_plugins(ChunkAuthorityPlugin);
        // …but a downstream validator rejects everything.
        app.add_chunk_change_validator(AlwaysRejectValidator);

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
    fn add_chunk_change_validator_inserts_resource_lazily() {
        // Calling the extension method before adding the plugin must
        // still work — the resource is inserted on demand.
        let mut app = App::new();
        app.add_chunk_change_validator(AlwaysRejectValidator);
        let validators = app.world().resource::<ChunkChangeValidators>();
        assert_eq!(validators.len(), 1);
    }

    #[test]
    fn first_rejecting_validator_short_circuits() {
        // If a downstream validator rejects, later validators must not
        // run. Use a counting validator after AlwaysReject and assert it
        // is never called.
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingValidator(Arc<AtomicUsize>);

        impl ChunkChangeValidator for CountingValidator {
            fn validate(
                &self,
                _world: &World,
                _change: &ChunkChange,
                _prior: Block,
                _chunk_pos: ChunkPos,
            ) -> CommitDecision {
                self.0.fetch_add(1, Ordering::SeqCst);
                CommitDecision::Accept
            }
        }

        let pos = ChunkPos::new(0, 0);
        let mut app = build_app_with_chunk(pos);
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(pos, ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));

        app.add_plugins(ChunkAuthorityPlugin);
        app.add_chunk_change_validator(AlwaysRejectValidator);
        let counter = Arc::new(AtomicUsize::new(0));
        app.add_chunk_change_validator(CountingValidator(counter.clone()));

        app.update();
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "downstream validator must not run after a reject"
        );
    }
}
