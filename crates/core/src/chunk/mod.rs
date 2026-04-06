use std::fmt::Display;

use bevy::{
    ecs::component::Component,
    prelude::{Deref, DerefMut},
    reflect::Reflect,
};
use serde::{Deserialize, Serialize, ser::SerializeTuple};

use crate::block::{Block, BlockCoord, BlockPos};

pub mod cache;
pub mod events;

/// Width (X) of a chunk in blocks.
pub const CHUNK_SIZE_X: usize = 16;
/// Height (Y) of a chunk in blocks.
pub const CHUNK_SIZE_Y: usize = 256;
/// Depth (Z) of a chunk in blocks.
pub const CHUNK_SIZE_Z: usize = 16;
/// Number of blocks in a chunk.
pub const CHUNK_SIZE: usize = CHUNK_SIZE_X * CHUNK_SIZE_Y * CHUNK_SIZE_Z;

/// Position of a chunk in the world, using chunk coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: BlockCoord,
    pub z: BlockCoord,
}

impl ChunkPos {
    pub fn new(x: BlockCoord, z: BlockCoord) -> Self {
        Self { x, z }
    }
}

impl Display for ChunkPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.z)
    }
}

#[derive(Clone, DerefMut, Deref)]
struct ChunkData([Block; CHUNK_SIZE]);

impl Serialize for ChunkData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_tuple(CHUNK_SIZE)?;
        for block in &self.0 {
            seq.serialize_element(block)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for ChunkData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ChunkDataVisitor;

        impl<'de> serde::de::Visitor<'de> for ChunkDataVisitor {
            type Value = ChunkData;

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut blocks = [Block::default(); CHUNK_SIZE];
                for i in 0..CHUNK_SIZE {
                    blocks[i] = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(ChunkData(blocks))
            }

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    formatter,
                    "expecting to find {} number of blocks",
                    CHUNK_SIZE
                )
            }
        }

        deserializer.deserialize_tuple(CHUNK_SIZE, ChunkDataVisitor)
    }
}

/// A chunk-sized slab of block data, optionally populated.
///
/// The flat array is indexed as:
///   `index = local_x + local_z * CHUNK_SIZE_X + local_y * CHUNK_SIZE_X * CHUNK_SIZE_Z`
#[derive(Clone, Serialize, Deserialize)]
pub struct Chunk {
    position: ChunkPos,
    data: Box<ChunkData>,
}

impl Chunk {
    /// Creates a new chunk at `position`, pre-filled with `Block::default()` (air).
    pub fn new(position: ChunkPos) -> Self {
        Self {
            position,
            data: Box::new(ChunkData([Block::default(); CHUNK_SIZE])),
        }
    }

    /// Returns the chunk's position in chunk coordinates.
    pub fn position(&self) -> ChunkPos {
        self.position
    }

    /// Returns the block at chunk-local coordinates, or `None` when the
    /// coordinates are out of range.
    pub fn get(&self, lx: usize, ly: usize, lz: usize) -> Option<Block> {
        if lx >= CHUNK_SIZE_X || ly >= CHUNK_SIZE_Y || lz >= CHUNK_SIZE_Z {
            return None;
        }
        Some(self.data[Self::index(lx, ly, lz)])
    }

    pub fn get_global(&self, pos: BlockPos) -> Option<Block> {
        if pos.x >= self.position.x
            || pos.x < self.position.x + CHUNK_SIZE_X as BlockCoord
            || pos.y >= CHUNK_SIZE_Y as BlockCoord
            || pos.z >= self.position.z
            || pos.z < self.position.z + CHUNK_SIZE_Z as BlockCoord
        {
            return None;
        };

        let local = pos.chunk_local();
        self.get(local.x as usize, local.y as usize, local.z as usize)
    }

    /// Sets the block at chunk-local coordinates. Does nothing when the
    /// coordinates are out of range.
    pub fn set(&mut self, lx: usize, ly: usize, lz: usize, block: Block) {
        if lx >= CHUNK_SIZE_X || ly >= CHUNK_SIZE_Y || lz >= CHUNK_SIZE_Z {
            return;
        }
        self.data[Self::index(lx, ly, lz)] = block;
    }

    /// Converts chunk-local coordinates to a flat array index.
    #[inline(always)]
    fn index(lx: usize, ly: usize, lz: usize) -> usize {
        lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z
    }
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunk")
            .field("position", &self.position)
            .finish()
    }
}
