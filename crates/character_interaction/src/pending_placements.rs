//! Per-actor queue of optimistic placements awaiting authoritative
//! confirmation — **server-only**.
//!
//! When [`try_place_block`][crate::placement::try_place_block] pushes a
//! predicted [`ChunkChange::Place`] onto the
//! [`ChunkCache`][dd40_core::chunk::cache::ChunkCache] *on the authoritative
//! instance*, it also records a [`PendingPlacement`] tagged with the placing
//! character.  Two server-side systems then react to the chunk-authority
//! pipeline:
//!
//! - [`consume_committed_placements`] runs after `ChunkAuthoritySet::Commit`
//!   and, for each matching [`ChunkChanged`] entry, decrements one item
//!   from the placing character's active inventory slot via
//!   [`InventoryComponent::decrement_active_slot`][dd40_inventory_core::component::InventoryComponent::decrement_active_slot].
//! - [`gc_pending_placements`] drops entries older than [`MAX_AGE_FRAMES`]
//!   — a safety valve so a chunk eviction or a silently-rejected
//!   prediction can't leak entries forever.
//!
//! The client never pushes onto this queue.  Its inventory is
//! server-replicated, so the moment that matters is the server's commit
//! pass; the client just observes the resulting `InventoryComponent`
//! change replicate back.  The whole module is gated on the presence of
//! [`PendingChunkRejections`][dd40_core::chunk::PendingChunkRejections]
//! (the same "authority lives here" marker the rest of this crate uses).
//!
//! Why a queue and not a single in-flight slot per character?  Multiple
//! placements can be in flight simultaneously — e.g. two adjacent
//! right-clicks across two ticks before the first reaches the
//! authority pass.  Matching is by `(chunk_pos, change)` so order
//! across actors doesn't matter; same-actor entries are still matched
//! oldest-first because [`Vec::iter().position`] returns the first hit.
//!
//! # Known caveat
//!
//! Server-side validator rejections currently leave the corresponding
//! entry in the queue until GC.  If actor A's place at `(x, y, z)` is
//! rejected and within `MAX_AGE_FRAMES` actor B successfully places at
//! the same cell, B's `ChunkChanged` will match A's stale entry first
//! and decrement A.  This is a narrow misattribution window; the fix
//! is to have `commit_predicted_changes` emit `PredictionRejected`
//! locally on server-side rejection too and drain matching entries
//! from a dedicated system.  Tracked but not addressed here.

use std::collections::VecDeque;

use bevy::prelude::*;
use dd40_core::chunk::events::ChunkChanged;
use dd40_core::chunk::{ChunkChange, ChunkPos};
use dd40_inventory_core::component::InventoryComponent;

/// Maximum number of frames a [`PendingPlacement`] may remain in the
/// queue before [`gc_pending_placements`] drops it.
///
/// At 60 Hz this is roughly ten seconds, well beyond the round-trip
/// time of a normal commit/reject reply.  Tuned conservatively because
/// the entry is small (`<64 B`) and a stale entry is harmless beyond
/// the memory cost.
pub const MAX_AGE_FRAMES: u32 = 600;

/// One in-flight predicted placement awaiting authoritative reply.
///
/// `actor` is the entity that owns the inventory to decrement once the
/// matching [`ChunkChanged`] arrives.  `chunk_pos` and `change` are how
/// the entry is matched against the authoritative reply.
#[derive(Debug, Clone)]
pub(crate) struct PendingPlacement {
    pub actor: Entity,
    pub chunk_pos: ChunkPos,
    pub change: ChunkChange,
    pub age: u32,
}

/// Per-world FIFO of in-flight placements awaiting confirmation.
///
/// Pushed by [`try_place_block`][crate::placement::try_place_block]
/// only on the authoritative instance (the server in a networked
/// build).  Drained by [`consume_committed_placements`] on commit and
/// by [`gc_pending_placements`] as the safety net.
#[derive(Resource, Default, Debug)]
pub(crate) struct PendingPlacements(pub VecDeque<PendingPlacement>);

/// Drains [`ChunkChanged`] and, for each authoritatively-committed
/// change that matches a queued [`PendingPlacement`], decrements one
/// item from the placing actor's active inventory slot.
///
/// Server-only — gated by the caller via the presence of
/// `PendingChunkRejections` (the chunk-authority marker).
pub(crate) fn consume_committed_placements(
    mut reader: MessageReader<ChunkChanged>,
    mut pending: ResMut<PendingPlacements>,
    mut inventories: Query<&mut InventoryComponent>,
    mut commands: Commands,
) {
    for msg in reader.read() {
        for change in &msg.changes {
            let Some(idx) = pending
                .0
                .iter()
                .position(|p| p.chunk_pos == msg.pos && &p.change == change)
            else {
                continue;
            };
            let entry = pending.0.remove(idx).expect("position came from iter");
            let Ok(mut inv) = inventories.get_mut(entry.actor) else {
                warn!(
                    "PendingPlacement actor {:?} has no InventoryComponent; \
                     placement committed but no item was decremented",
                    entry.actor
                );
                continue;
            };
            inv.decrement_active_slot(1, &mut commands, entry.actor);
        }
    }
}

/// Ages every queued entry by one frame and evicts entries older than
/// [`MAX_AGE_FRAMES`].
///
/// Server-only — gated by the caller via the presence of
/// `PendingChunkRejections`.
pub(crate) fn gc_pending_placements(mut pending: ResMut<PendingPlacements>) {
    pending.0.retain_mut(|p| {
        p.age = p.age.saturating_add(1);
        p.age < MAX_AGE_FRAMES
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::chunk::{BlockLocal, ChunkChange};
    use dd40_core::prelude::{BlockId, ChunkPos};

    #[test]
    fn gc_drops_entries_at_max_age() {
        let mut app = App::new();
        app.init_resource::<PendingPlacements>()
            .add_systems(Update, gc_pending_placements);

        let chunk = ChunkPos::new(0, 0, 0);
        app.world_mut()
            .resource_mut::<PendingPlacements>()
            .0
            .push_back(PendingPlacement {
                actor: Entity::PLACEHOLDER,
                chunk_pos: chunk,
                change: ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(1)),
                age: MAX_AGE_FRAMES - 1,
            });

        app.update();
        let pending = app.world().resource::<PendingPlacements>();
        assert!(pending.0.is_empty(), "entry at MAX_AGE should be evicted");
    }

    #[test]
    fn gc_keeps_young_entries() {
        let mut app = App::new();
        app.init_resource::<PendingPlacements>()
            .add_systems(Update, gc_pending_placements);

        let chunk = ChunkPos::new(0, 0, 0);
        app.world_mut()
            .resource_mut::<PendingPlacements>()
            .0
            .push_back(PendingPlacement {
                actor: Entity::PLACEHOLDER,
                chunk_pos: chunk,
                change: ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(1)),
                age: 0,
            });

        app.update();
        let pending = app.world().resource::<PendingPlacements>();
        assert_eq!(pending.0.len(), 1);
        assert_eq!(pending.0[0].age, 1);
    }
}
