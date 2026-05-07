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

use bevy::prelude::*;

use crate::block::{Block, BlockRegistry};
use crate::chunk::{
    ChunkChange, PredictedChange,
    cache::ChunkCache,
    events::ChunkChanged,
};

/// Outcome of validating one predicted change against its chunk's
/// pre-prediction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitDecision {
    /// The change passed validation and may be applied.
    Accept,
    /// The change failed validation. The associated reason is logged by the
    /// caller at `warn!` level.
    Reject(RejectReason),
}

/// Why a predicted change was rejected at commit time.
///
/// Always logged at `warn!` by the commit system. Silence is wrong — every
/// rejection is either a bug, a desync, or a malicious client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// `Place` targeted a non-replaceable cell.
    NotReplaceable,
    /// `Remove` targeted an indestructible cell (or air).
    NotDestructible,
}

/// Pure validator: decide whether `change` is acceptable, given that
/// `prior` was the block in the target cell **before** the change was
/// optimistically applied.
///
/// `Replace` is unconditional and always accepted (used by world
/// generation, redstone, etc.).
pub fn validate_change(
    change: &ChunkChange,
    prior: Block,
    registry: &BlockRegistry,
) -> CommitDecision {
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

/// System: drain every chunk's predicted queue, validate, apply, and emit
/// [`ChunkChanged`].
///
/// Runs in `PostUpdate` only when [`ChunkAuthorityPlugin`] is added.
///
/// **Rejection semantics.** When a predicted change fails validation, the
/// cell is rolled back to the `prior` value captured at push time and a
/// `warn!` log line is emitted with chunk position, cell, and reason.
pub fn commit_predicted_changes(
    mut cache: ResMut<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut chunk_changed: MessageWriter<ChunkChanged>,
) {
    let positions: Vec<_> = cache.iter_positions().copied().collect();

    for pos in positions {
        let Some(chunk) = cache.get_mut(&pos) else {
            continue;
        };
        if chunk.predicted().is_empty() {
            continue;
        }

        let predicted: Vec<PredictedChange> = chunk.take_predicted();
        let mut accepted: Vec<ChunkChange> = Vec::with_capacity(predicted.len());

        for entry in predicted {
            match validate_change(&entry.change, entry.prior, &registry) {
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
        let changes: Vec<ChunkChange> = committed.into_iter().map(|(_, c)| c).collect();

        chunk_changed.write(ChunkChanged {
            pos,
            changes,
            new_version,
        });
    }
}

/// Server-only Bevy plugin that adds the authoritative
/// [`commit_predicted_changes`] system in [`PostUpdate`].
///
/// Adding this plugin promotes the local instance to chunk authority. The
/// server binary adds it; the client never does.
#[derive(Default)]
pub struct ChunkAuthorityPlugin;

impl Plugin for ChunkAuthorityPlugin {
    fn build(&self, app: &mut App) {
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

    #[test]
    fn validate_accepts_place_into_air() {
        let r = registry_with_stone();
        let c = ChunkChange::new_place(lp(0, 0, 0), BlockId(1));
        assert_eq!(validate_change(&c, Block::default(), &r), CommitDecision::Accept);
    }

    #[test]
    fn validate_rejects_place_into_solid() {
        let r = registry_with_stone();
        let c = ChunkChange::new_place(lp(0, 0, 0), BlockId(1));
        assert_eq!(
            validate_change(&c, Block::new(BlockId(1)), &r),
            CommitDecision::Reject(RejectReason::NotReplaceable),
        );
    }

    #[test]
    fn validate_accepts_remove_of_destructible() {
        let r = registry_with_stone();
        let c = ChunkChange::new_remove(lp(0, 0, 0));
        assert_eq!(
            validate_change(&c, Block::new(BlockId(1)), &r),
            CommitDecision::Accept,
        );
    }

    #[test]
    fn validate_rejects_remove_of_air() {
        let r = registry_with_stone();
        let c = ChunkChange::new_remove(lp(0, 0, 0));
        assert_eq!(
            validate_change(&c, Block::default(), &r),
            CommitDecision::Reject(RejectReason::NotDestructible),
        );
    }

    #[test]
    fn validate_replace_is_always_accepted() {
        let r = registry_with_stone();
        let c = ChunkChange::new_replace(lp(0, 0, 0), BlockId(1));
        assert_eq!(validate_change(&c, Block::new(BlockId(1)), &r), CommitDecision::Accept);
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
        // Cell was rolled back to its pre-prediction value (stone).
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
}
