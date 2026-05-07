//! Mutation events that describe how a [`Chunk`](super::Chunk) changes.
//!
//! Every mutation to a chunk (by world generation, the player, the network,
//! redstone, fire, ...) flows through a single type: [`ChunkChange`]. A
//! `ChunkChange` carries chunk-local coordinates only — a chunk has no
//! knowledge of the global world position it is mounted at, and a chunk
//! can be physically moved between [`ChunkPos`](super::ChunkPos)es without
//! rewriting any of its inner data.
//!
//! Two queues of `ChunkChange` live on each chunk:
//!
//! - `predicted` — local, optimistic mutations that have not yet been
//!   acknowledged by the authoritative server. They are applied to the
//!   chunk's data immediately so the local renderer reflects the optimistic
//!   state, and rolled back on rejection.
//! - `confirmed_history` — server-authoritative mutations, paired with the
//!   chunk version they produced. The history is uncapped in memory and is
//!   only dropped when the chunk is evicted from the cache.

use serde::{Deserialize, Serialize};

use crate::block::BlockId;

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
        Self::try_new(x, y, z)
            .unwrap_or_else(|| panic!("BlockLocal out of range: ({x}, {y}, {z})"))
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
            ChunkChange::Place { local: l, block_id: id },
        );
        assert_eq!(ChunkChange::new_remove(l), ChunkChange::Remove { local: l });
        assert_eq!(
            ChunkChange::new_replace(l, id),
            ChunkChange::Replace { local: l, new_block: id },
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
            let decoded: ChunkChange =
                bincode::deserialize(&bytes).expect("deserialize");
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
}
