//! Mutation events that describe how a [`Chunk`](super::Chunk) changes.
//!
//! Every mutation to a chunk's **block array** (by world generation, the
//! player, the network, redstone, fire, ...) flows through a single type:
//! [`ChunkChange`]. A `ChunkChange` carries chunk-local coordinates only —
//! a chunk has no knowledge of the global world position it is mounted at,
//! and a chunk can be physically moved between
//! [`ChunkPos`](super::ChunkPos)es without rewriting any of its inner data.
//!
//! Mutations to the **per-cell typed data** store (chest contents, sign
//! text, bed bindings — anything attached via
//! [`BlockData`]) flow through a parallel type:
//! [`CellDataChange`]. They run through the same authority pipeline as
//! [`ChunkChange`] and share the chunk's version counter, but the two
//! queues stay separate because [`CellDataChange`] carries `Box<dyn
//! BlockData>` and so cannot be `Copy`/`Serialize`/`Eq` — keeping it out
//! of [`ChunkChange`] preserves the latter's small, fixed-size wire form.
//!
//! Two queues of each kind live on every chunk:
//!
//! - `predicted` — local, optimistic mutations that have not yet been
//!   acknowledged by the authoritative server. They are applied to the
//!   chunk's data immediately so the local renderer reflects the optimistic
//!   state, and rolled back on rejection.
//! - `confirmed_history` — server-authoritative mutations, paired with the
//!   chunk version they produced. The history is uncapped in memory and is
//!   only dropped when the chunk is evicted from the cache.

use std::any::TypeId;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::block::{BlockData, BlockId};

/// Chunk-local block coordinate.
///
/// `x` and `z` are bounded by [`CHUNK_SIZE_X`](super::CHUNK_SIZE_X) /
/// [`CHUNK_SIZE_Z`](super::CHUNK_SIZE_Z) (16 each), `y` by
/// [`CHUNK_SIZE_Y`](super::CHUNK_SIZE_Y) (256). The compact in-memory layout
/// keeps every `ChunkChange` small.
///
/// Construct with [`BlockLocal::new`] (panics on out-of-range) or
/// [`BlockLocal::try_new`] (returns `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockLocal {
    /// X coordinate within the chunk: `0..CHUNK_SIZE_X`.
    pub x: u8,
    /// Y coordinate within the chunk: `0..CHUNK_SIZE_Y`.
    pub y: u16,
    /// Z coordinate within the chunk: `0..CHUNK_SIZE_Z`.
    pub z: u8,
}

impl BlockLocal {
    /// Creates a new chunk-local position.
    ///
    /// # Panics
    ///
    /// Panics if any coordinate is outside the chunk bounds.
    #[inline]
    pub fn new(x: u8, y: u16, z: u8) -> Self {
        Self::try_new(x, y, z).unwrap_or_else(|| panic!("BlockLocal out of range: ({x}, {y}, {z})"))
    }

    /// Creates a new chunk-local position, returning `None` if any coordinate
    /// is outside the chunk bounds.
    #[inline]
    pub fn try_new(x: u8, y: u16, z: u8) -> Option<Self> {
        if (x as usize) < super::CHUNK_SIZE_X
            && (y as usize) < super::CHUNK_SIZE_Y
            && (z as usize) < super::CHUNK_SIZE_Z
        {
            Some(Self { x, y, z })
        } else {
            None
        }
    }
}

/// A single authoritative or predicted mutation to a [`Chunk`](super::Chunk).
///
/// The variant determines what happens at apply time:
///
/// - [`ChunkChange::Place`] — the cell **must** currently hold a replaceable
///   block (typically air). Rejected otherwise.
/// - [`ChunkChange::Remove`] — the cell **must** currently hold a destructible
///   non-air block. Rejected otherwise.
/// - [`ChunkChange::Replace`] — unconditional swap. Used by world generation,
///   redstone, and other systems that don't care about the prior block.
///
/// All coordinates are chunk-local. New mutation kinds (e.g. metadata
/// updates, fluid level changes) get added to this enum rather than as new
/// network messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkChange {
    /// Place a block into a replaceable cell.
    Place {
        /// Cell to write to.
        local: BlockLocal,
        /// Block id to place.
        block_id: BlockId,
    },
    /// Remove (set to [`BlockId::AIR`](crate::block::BlockId::AIR)) a destructible block.
    Remove {
        /// Cell to clear.
        local: BlockLocal,
    },
    /// Unconditional replacement. Skips the replaceable / destructible check.
    Replace {
        /// Cell to overwrite.
        local: BlockLocal,
        /// Block id to write.
        new_block: BlockId,
    },
}

impl ChunkChange {
    /// Convenience constructor for [`ChunkChange::Place`].
    #[inline]
    pub fn new_place(local: BlockLocal, block_id: BlockId) -> Self {
        Self::Place { local, block_id }
    }

    /// Convenience constructor for [`ChunkChange::Remove`].
    #[inline]
    pub fn new_remove(local: BlockLocal) -> Self {
        Self::Remove { local }
    }

    /// Convenience constructor for [`ChunkChange::Replace`].
    #[inline]
    pub fn new_replace(local: BlockLocal, new_block: BlockId) -> Self {
        Self::Replace { local, new_block }
    }

    /// Returns the chunk-local cell this change targets.
    #[inline]
    pub fn local(&self) -> BlockLocal {
        match *self {
            ChunkChange::Place { local, .. }
            | ChunkChange::Remove { local }
            | ChunkChange::Replace { local, .. } => local,
        }
    }
}

/// A single authoritative or predicted mutation to a chunk's **per-cell
/// typed data** store (see [`Chunk::cell_data`](super::Chunk::cell_data)).
///
/// Distinct from [`ChunkChange`] because the payload is a `Box<dyn
/// BlockData>` — heap-allocated, not `Copy`, not directly `Serialize`. It
/// flows through the same authority pipeline as [`ChunkChange`] (validate
/// → commit → version-bump → emit
/// [`ChunkChanged`](super::events::ChunkChanged)) and shares the chunk's
/// version counter, so a single commit pass produces a unified history
/// regardless of which queue an entry came from.
///
/// Wire/disk transport is handled by `NetworkedCellDataChange` (forthcoming
/// in S5) and the storage `V2` format (S7), both of which serialise through
/// [`BlockDataTypeRegistry`](crate::block::BlockDataTypeRegistry).
///
/// All coordinates are chunk-local.
pub enum CellDataChange {
    /// Insert or replace the `T`-typed value at `local` with `value`.
    /// `value.as_any().type_id()` must match `value.type_key()` and must
    /// be a registered type in `BlockDataTypeRegistry`.
    Set {
        /// Cell to write to.
        local: BlockLocal,
        /// Value to insert; replaces any prior value of the same
        /// [`TypeId`] at the cell.
        value: Box<dyn BlockData>,
    },
    /// Remove the value of the given type at `local`, if any.
    Clear {
        /// Cell to clear at.
        local: BlockLocal,
        /// Concrete type of the value to remove.  Carried as a
        /// [`TypeId`] for the in-memory lookup; `type_key` is the
        /// stable `type_name`-style identifier used by the wire/disk
        /// formats to round-trip the same change through
        /// `BlockDataTypeRegistry`.
        type_id: TypeId,
        /// Stable string identifier for the type — typically the value
        /// returned by [`BlockData::type_key`] of the concrete type.
        type_key: &'static str,
    },
}

impl CellDataChange {
    /// Convenience constructor for [`CellDataChange::Set`].
    pub fn new_set<T: BlockData>(local: BlockLocal, value: T) -> Self {
        Self::Set {
            local,
            value: Box::new(value),
        }
    }

    /// Convenience constructor for [`CellDataChange::Clear`].
    pub fn new_clear<T: BlockData>(local: BlockLocal) -> Self {
        Self::Clear {
            local,
            type_id: TypeId::of::<T>(),
            type_key: std::any::type_name::<T>(),
        }
    }

    /// Builds a [`CellDataChange::Clear`] from a raw `(TypeId, type_key)`
    /// pair.
    ///
    /// Used by the authority commit pass when synthesising cleanup
    /// changes for blocks that no longer exist — the caller doesn't have
    /// the concrete type in scope, only the runtime identifiers it
    /// drained out of `Chunk::drain_cell_data_at`.
    pub fn clear_raw(local: BlockLocal, type_id: TypeId, type_key: &'static str) -> Self {
        Self::Clear {
            local,
            type_id,
            type_key,
        }
    }

    /// Returns the chunk-local cell this change targets.
    #[inline]
    pub fn local(&self) -> BlockLocal {
        match self {
            CellDataChange::Set { local, .. } | CellDataChange::Clear { local, .. } => *local,
        }
    }

    /// Returns the [`TypeId`] of the [`BlockData`] type this change
    /// targets.  For [`CellDataChange::Set`] this is the runtime type of
    /// `value`; for [`CellDataChange::Clear`] it is the type recorded at
    /// construction time.
    #[inline]
    pub fn type_id(&self) -> TypeId {
        match self {
            CellDataChange::Set { value, .. } => value.as_any().type_id(),
            CellDataChange::Clear { type_id, .. } => *type_id,
        }
    }

    /// Returns the stable string identifier for the [`BlockData`] type
    /// this change targets.  See [`BlockData::type_key`].
    #[inline]
    pub fn type_key(&self) -> &'static str {
        match self {
            CellDataChange::Set { value, .. } => value.type_key(),
            CellDataChange::Clear { type_key, .. } => type_key,
        }
    }
}

impl Clone for CellDataChange {
    fn clone(&self) -> Self {
        match self {
            CellDataChange::Set { local, value } => CellDataChange::Set {
                local: *local,
                value: value.clone_box(),
            },
            CellDataChange::Clear {
                local,
                type_id,
                type_key,
            } => CellDataChange::Clear {
                local: *local,
                type_id: *type_id,
                type_key,
            },
        }
    }
}

impl fmt::Debug for CellDataChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellDataChange::Set { local, value } => f
                .debug_struct("Set")
                .field("local", local)
                .field("type_key", &value.type_key())
                .finish(),
            CellDataChange::Clear {
                local, type_key, ..
            } => f
                .debug_struct("Clear")
                .field("local", local)
                .field("type_key", type_key)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_local_in_range_is_some() {
        assert!(BlockLocal::try_new(0, 0, 0).is_some());
        assert!(BlockLocal::try_new(15, 255, 15).is_some());
    }

    #[test]
    fn block_local_out_of_range_is_none() {
        assert!(BlockLocal::try_new(16, 0, 0).is_none());
        assert!(BlockLocal::try_new(0, 256, 0).is_none());
        assert!(BlockLocal::try_new(0, 0, 16).is_none());
    }

    #[test]
    #[should_panic]
    fn block_local_new_panics_out_of_range() {
        let _ = BlockLocal::new(16, 0, 0);
    }

    #[test]
    fn constructors_set_expected_variant_and_local() {
        let l = BlockLocal::new(1, 2, 3);
        let id = BlockId(42);

        assert_eq!(
            ChunkChange::new_place(l, id),
            ChunkChange::Place {
                local: l,
                block_id: id
            },
        );
        assert_eq!(ChunkChange::new_remove(l), ChunkChange::Remove { local: l });
        assert_eq!(
            ChunkChange::new_replace(l, id),
            ChunkChange::Replace {
                local: l,
                new_block: id
            },
        );

        assert_eq!(ChunkChange::new_place(l, id).local(), l);
        assert_eq!(ChunkChange::new_remove(l).local(), l);
        assert_eq!(ChunkChange::new_replace(l, id).local(), l);
    }

    #[test]
    fn serde_round_trip_all_variants() {
        let l = BlockLocal::new(7, 64, 9);
        let id = BlockId(123);

        let cases = [
            ChunkChange::new_place(l, id),
            ChunkChange::new_remove(l),
            ChunkChange::new_replace(l, id),
        ];

        for original in cases {
            let bytes = bincode::serialize(&original).expect("serialize");
            let decoded: ChunkChange = bincode::deserialize(&bytes).expect("deserialize");
            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn block_local_serde_round_trip() {
        let l = BlockLocal::new(5, 200, 10);
        let bytes = bincode::serialize(&l).expect("serialize");
        let decoded: BlockLocal = bincode::deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded, l);
    }

    // -----------------------------------------------------------------
    // CellDataChange
    // -----------------------------------------------------------------

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct ChestState {
        slots: u32,
    }
    impl crate::block::BlockData for ChestState {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn crate::block::BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct SignText(String);
    impl crate::block::BlockData for SignText {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn crate::block::BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn cell_data_change_set_reports_local_typeid_and_key() {
        let l = BlockLocal::new(3, 7, 11);
        let c = CellDataChange::new_set(l, ChestState { slots: 27 });
        assert_eq!(c.local(), l);
        assert_eq!(c.type_id(), TypeId::of::<ChestState>());
        assert_eq!(c.type_key(), std::any::type_name::<ChestState>());
    }

    #[test]
    fn cell_data_change_clear_reports_local_typeid_and_key() {
        let l = BlockLocal::new(0, 0, 0);
        let c = CellDataChange::new_clear::<SignText>(l);
        assert_eq!(c.local(), l);
        assert_eq!(c.type_id(), TypeId::of::<SignText>());
        assert_eq!(c.type_key(), std::any::type_name::<SignText>());
    }

    #[test]
    fn cell_data_change_clone_preserves_payload() {
        let l = BlockLocal::new(1, 2, 3);
        let original = CellDataChange::new_set(l, ChestState { slots: 5 });
        let cloned = original.clone();
        assert_eq!(cloned.local(), original.local());
        assert_eq!(cloned.type_id(), original.type_id());
        // Down-cast to verify the cloned payload survived intact.
        let CellDataChange::Set { value, .. } = cloned else {
            panic!("Set expected");
        };
        let chest = value
            .as_any()
            .downcast_ref::<ChestState>()
            .expect("ChestState downcast");
        assert_eq!(chest.slots, 5);
    }

    #[test]
    fn cell_data_change_debug_redacts_payload() {
        let l = BlockLocal::new(0, 0, 0);
        let c = CellDataChange::new_set(l, ChestState { slots: 99 });
        let s = format!("{c:?}");
        assert!(s.contains("Set"), "debug should mention variant: {s}");
        assert!(s.contains("ChestState"), "debug should mention type: {s}");
    }
}
