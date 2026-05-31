//! Per-actor queue of optimistic placements awaiting authoritative
//! confirmation.
//!
//! When [`try_place_block`][crate::placement::try_place_block] pushes a
//! predicted [`ChunkChange::Place`] onto the
//! [`ChunkCache`][dd40_core::chunk::cache::ChunkCache], it also records a
//! [`PendingPlacement`] tagged with the placing character.  Two server-side
//! systems then react to the chunk-authority pipeline:
//!
//! - [`consume_committed_placements`] runs after `ChunkAuthoritySet::Commit`
//!   and, for each matching [`ChunkChanged`] entry, decrements one item
//!   from the placing character's active inventory slot via
//!   [`InventoryComponent::decrement_active_slot`][dd40_inventory_core::component::InventoryComponent::decrement_active_slot].
//! - [`drop_rejected_placements`] removes entries whose corresponding
//!   prediction was rejected (the `ChunkChange` never made it through
//!   validation), so the player keeps the item.
//!
//! [`gc_pending_placements`] runs everywhere and drops entries older
//! than [`MAX_AGE_FRAMES`] frames — a safety valve so a network drop or
//! a chunk eviction can't leak entries forever.  On clients (which run
//! the predictor but not the commit/reject pipeline) the GC is the only
//! cleanup, so the queue stays bounded.
//!
//! Why a queue and not a single in-flight slot per character?  Multiple
//! placements can be in flight simultaneously — e.g. two adjacent
//! right-clicks across two ticks before the first reaches the
//! authority pass.  Matching is by `(chunk_pos, change)` so order
//! across actors doesn't matter; same-actor entries are still matched
//! oldest-first because [`Vec::iter().position`] returns the first hit.

use std::collections::VecDeque;

use bevy::prelude::*;
use dd40_core::chunk::events::{ChunkChanged, PredictionRejected};
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
/// every time a predicted [`ChunkChange::Place`] lands in the chunk
/// cache.  Drained on the server by [`consume_committed_placements`]
/// (success) and [`drop_rejected_placements`] (rejection); GC'd
/// everywhere by [`gc_pending_placements`].
#[derive(Resource, Default, Debug)]
pub(crate) struct PendingPlacements(pub VecDeque<PendingPlacement>);

/// Drains [`ChunkChanged`] and, for each authoritatively-committed
/// change that matches a queued [`PendingPlacement`], decrements one
/// item from the placing actor's active inventory slot.
///
/// Server-only — gated by the caller via the presence of
/// `PendingChunkRejections` (the chunk-authority marker).  On the
/// client the same `ChunkChanged` messages fire after reconciling a
/// `ChunkUpdate`, but the client's inventory is server-replicated and
/// must not be mutated locally.
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

/// Drains [`PredictionRejected`] and removes any matching
/// [`PendingPlacement`] entry without touching the inventory.
///
/// Runs on the client (the only side that emits `PredictionRejected`).
pub(crate) fn drop_rejected_placements(
    mut reader: MessageReader<PredictionRejected>,
    mut pending: ResMut<PendingPlacements>,
) {
    for msg in reader.read() {
        if let Some(idx) = pending
            .0
            .iter()
            .position(|p| p.chunk_pos == msg.pos && p.change == msg.change)
        {
            pending.0.remove(idx);
        }
    }
}

/// Ages every queued entry by one frame and evicts entries older than
/// [`MAX_AGE_FRAMES`].
///
/// Runs on both client and server — on the client it is the only
/// cleanup path (nothing else drains entries), on the server it backs
/// up the commit/reject paths for the rare case a chunk is evicted
/// before either fires.
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

    fn place(actor: Entity, chunk: ChunkPos, x: u8, y: u16, z: u8, id: u16) -> PendingPlacement {
        PendingPlacement {
            actor,
            chunk_pos: chunk,
            change: ChunkChange::new_place(BlockLocal::new(x, y, z), BlockId(id)),
            age: 0,
        }
    }

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

        // One tick → age becomes MAX_AGE_FRAMES → evicted.
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
            .push_back(place(Entity::PLACEHOLDER, chunk, 0, 0, 0, 1));

        app.update();
        let pending = app.world().resource::<PendingPlacements>();
        assert_eq!(pending.0.len(), 1);
        assert_eq!(pending.0[0].age, 1);
    }

    #[test]
    fn drop_rejected_removes_matching_entry_only() {
        let mut app = App::new();
        app.init_resource::<PendingPlacements>()
            .add_message::<PredictionRejected>()
            .add_systems(Update, drop_rejected_placements);

        let chunk = ChunkPos::new(0, 0, 0);
        let other_chunk = ChunkPos::new(1, 0, 0);
        let actor = Entity::PLACEHOLDER;
        {
            let mut q = app.world_mut().resource_mut::<PendingPlacements>();
            q.0.push_back(place(actor, chunk, 0, 0, 0, 1));
            q.0.push_back(place(actor, other_chunk, 0, 0, 0, 1));
        }

        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<PredictionRejected>>()
            .write(PredictionRejected {
                pos: chunk,
                change: ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(1)),
            });

        app.update();
        let pending = app.world().resource::<PendingPlacements>();
        assert_eq!(pending.0.len(), 1);
        assert_eq!(pending.0[0].chunk_pos, other_chunk);
    }

    #[test]
    fn drop_rejected_no_match_keeps_queue() {
        let mut app = App::new();
        app.init_resource::<PendingPlacements>()
            .add_message::<PredictionRejected>()
            .add_systems(Update, drop_rejected_placements);

        let chunk = ChunkPos::new(0, 0, 0);
        let actor = Entity::PLACEHOLDER;
        app.world_mut()
            .resource_mut::<PendingPlacements>()
            .0
            .push_back(place(actor, chunk, 0, 0, 0, 1));

        // Different block_id → no match.
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<PredictionRejected>>()
            .write(PredictionRejected {
                pos: chunk,
                change: ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(99)),
            });

        app.update();
        assert_eq!(app.world().resource::<PendingPlacements>().0.len(), 1);
    }
}
