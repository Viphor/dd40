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
/// Validators may freely access the Bevy [`World`] to read whatever
/// resources or queries they need. They are called from an exclusive
/// system, so there is no contention to worry about.
///
/// Validators are stateful (`&mut self`) so they can cache
/// [`SystemState`](bevy::ecs::system::SystemState) or other per-validator
/// resources across calls.
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
///         &mut self,
///         _world: &mut World,
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
        &mut self,
        world: &mut World,
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
        &mut self,
        world: &mut World,
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
/// Returns the first [`CommitDecision::Reject`] produced; otherwise
/// returns [`CommitDecision::Accept`]. The chain is briefly removed
/// from the world during the call so that validators may freely
/// access the world themselves.
fn run_validators(
    world: &mut World,
    change: &ChunkChange,
    prior: Block,
    chunk_pos: ChunkPos,
) -> CommitDecision {
    let Some(mut validators) = world.remove_resource::<ChunkChangeValidators>() else {
        return CommitDecision::Accept;
    };

    let mut decision = CommitDecision::Accept;
    for validator in validators.validators.iter_mut() {
        decision = validator.validate(world, change, prior, chunk_pos);
        if matches!(decision, CommitDecision::Reject(_)) {
            break;
        }
    }

    world.insert_resource(validators);
    decision
}

/// Exclusive system: drain every chunk's predicted queue, run it
/// through the registered validator chain, apply accepted changes,
/// roll back rejected ones, and emit [`ChunkChanged`] messages.
///
/// Runs in [`PostUpdate`] when [`ChunkAuthorityPlugin`] is added.
pub fn commit_predicted_changes(world: &mut World) {
    let positions: Vec<ChunkPos> = world
        .resource::<ChunkCache>()
        .iter_positions()
        .copied()
        .collect();

    for pos in positions {
        let predicted: Vec<PredictedChange> = {
            let mut cache = world.resource_mut::<ChunkCache>();
            let Some(chunk) = cache.get_mut(&pos) else {
                continue;
            };
            if chunk.predicted().is_empty() {
                continue;
            }
            chunk.take_predicted()
        };

        let mut accepted: Vec<ChunkChange> = Vec::with_capacity(predicted.len());

        for entry in predicted {
            let decision = run_validators(world, &entry.change, entry.prior, pos);
            match decision {
                CommitDecision::Accept => accepted.push(entry.change),
                CommitDecision::Reject(reason) => {
                    warn!(
                        "Rejected predicted change at chunk {} cell {:?}: {:?}",
                        pos,
                        entry.change.local(),
                        reason,
                    );
                    let mut cache = world.resource_mut::<ChunkCache>();
                    if let Some(chunk) = cache.get_mut(&pos) {
                        chunk.rollback_to(entry.change.local(), entry.prior);
                    }
                }
            }
        }

        if accepted.is_empty() {
            continue;
        }

        let (changes, new_version) = {
            let mut cache = world.resource_mut::<ChunkCache>();
            let Some(chunk) = cache.get_mut(&pos) else {
                continue;
            };
            let committed = chunk.commit_accepted(&accepted);
            (
                committed.into_iter().map(|(_, c)| c).collect::<Vec<_>>(),
                chunk.version(),
            )
        };

        world.resource_mut::<Messages<ChunkChanged>>().write(
            ChunkChanged {
                pos,
                changes,
                new_version,
            },
        );
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

    /// Helper: run the default validator against a single change without
    /// going through a full Bevy app. Builds a throwaway world, registers
    /// the registry, and calls the validator directly.
    fn run_default(change: &ChunkChange, prior: Block, registry: BlockRegistry) -> CommitDecision {
        let mut world = World::new();
        world.insert_resource(registry);
        let mut v = DefaultBlockRegistryValidator;
        v.validate(&mut world, change, prior, ChunkPos::new(0, 0))
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

    #[test]
    fn plugin_drains_predicted_and_emits_chunk_changed() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.push_predicted(ChunkChange::new_place(lp(2, 3, 4), BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).expect("chunk present");
        assert_eq!(chunk.version(), 1);
        assert!(chunk.predicted().is_empty());
        assert_eq!(chunk.confirmed_history().len(), 1);

        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<ChunkChanged>>();
        let emitted: Vec<_> = messages.iter_current_update_messages().cloned().collect();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].new_version, 1);
        assert_eq!(emitted[0].changes.len(), 1);
        assert_eq!(emitted[0].pos, ChunkPos::new(0, 0));
    }

    #[test]
    fn plugin_rolls_back_rejected_prediction() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Pre-fill the cell with stone (non-replaceable). This is the
        // pre-prediction state.
        chunk.set_local(lp(0, 0, 0), Block::new(BlockId(1)));
        // Push a Place — the prior captured by push_predicted is stone,
        // so validation rejects the change.
        chunk.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        assert_eq!(chunk.version(), 0);
        assert!(chunk.predicted().is_empty());
        assert_eq!(chunk.confirmed_history().len(), 0);
        assert_eq!(chunk.get_local(lp(0, 0, 0)).block_id, BlockId(1));
    }

    #[test]
    fn predicted_queue_untouched_without_plugin() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        // No ChunkAuthorityPlugin — predicted queue must survive.
        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        assert_eq!(chunk.predicted().len(), 1);
        assert_eq!(chunk.version(), 0);
    }

    /// A custom validator that rejects every change with a custom reason.
    /// Used to verify the extension contract.
    struct AlwaysRejectValidator;

    impl ChunkChangeValidator for AlwaysRejectValidator {
        fn validate(
            &mut self,
            _world: &mut World,
            _change: &ChunkChange,
            _prior: Block,
            _chunk_pos: ChunkPos,
        ) -> CommitDecision {
            CommitDecision::Reject(RejectReason::custom("test rejection"))
        }
    }

    #[test]
    fn registered_custom_validator_is_consulted() {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Push a change the default validator would ACCEPT (place into air)…
        chunk.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        // …but a downstream validator rejects everything.
        app.add_chunk_change_validator(AlwaysRejectValidator);

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        // Rolled back: cell stays air, version stays 0, no history, no
        // ChunkChanged message.
        assert_eq!(chunk.version(), 0);
        assert!(chunk.confirmed_history().is_empty());
        assert_eq!(chunk.get_local(lp(0, 0, 0)).block_id, BlockId::AIR);

        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<ChunkChanged>>();
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
                &mut self,
                _world: &mut World,
                _change: &ChunkChange,
                _prior: Block,
                _chunk_pos: ChunkPos,
            ) -> CommitDecision {
                self.0.fetch_add(1, Ordering::SeqCst);
                CommitDecision::Accept
            }
        }

        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_stone());

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.add_chunk_change_validator(AlwaysRejectValidator);
        let counter = Arc::new(AtomicUsize::new(0));
        app.add_chunk_change_validator(CountingValidator(counter.clone()));

        app.update();
        assert_eq!(counter.load(Ordering::SeqCst), 0, "downstream validator must not run after a reject");
    }
}
