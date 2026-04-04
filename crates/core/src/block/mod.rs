use bevy::{ecs::component::Component, reflect::Reflect, transform::components::Transform};
use serde::{Deserialize, Serialize};

pub mod events;
pub mod registry;
//pub mod storage;

pub use registry::{BlockDefinition, BlockRegistry};

use crate::chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Z, ChunkPos};

pub type BlockCoord = i32;

/// Global integer block position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize)]
pub struct BlockPos {
    pub x: BlockCoord,
    pub y: BlockCoord,
    pub z: BlockCoord,
}

impl BlockPos {
    pub fn new(x: BlockCoord, y: BlockCoord, z: BlockCoord) -> Self {
        Self { x, y, z }
    }

    /// Returns the chunk-space position that contains this block.
    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos {
            x: self.x.div_euclid(CHUNK_SIZE_X as BlockCoord),
            z: self.z.div_euclid(CHUNK_SIZE_Z as BlockCoord),
        }
    }

    /// Returns the position within the chunk.
    pub fn chunk_local(&self) -> Self {
        Self {
            x: self.x.rem_euclid(CHUNK_SIZE_X as BlockCoord),
            y: self.y,
            z: self.z.rem_euclid(CHUNK_SIZE_Z as BlockCoord),
        }
    }
}

impl From<&Transform> for BlockPos {
    fn from(value: &Transform) -> Self {
        Self {
            x: value.translation.x.floor() as BlockCoord,
            y: value.translation.y.floor() as BlockCoord,
            z: value.translation.z.floor() as BlockCoord,
        }
    }
}

impl std::fmt::Display for BlockPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

/// A unique identifier for a block type.
/// Uses a u16 to allow up to 65,536 different block types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize)]
pub struct BlockId(pub u16);

impl BlockId {
    /// Air block (ID 0) - always registered by default.
    pub const AIR: BlockId = BlockId(0);
}

/// A single block, storing its type.
#[derive(Debug, Clone, Copy, Component, Reflect, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub block_id: BlockId,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            block_id: BlockId::AIR,
        }
    }
}

impl Block {
    pub fn new(block_id: BlockId) -> Self {
        Self { block_id }
    }

    /// Checks if this block is solid by looking it up in the registry.
    pub fn is_solid(&self, registry: &BlockRegistry) -> bool {
        registry
            .get(self.block_id)
            .map(|def| def.is_solid)
            .unwrap_or(false)
    }

    /// Checks if this block is renderable by looking it up in the registry.
    pub fn is_renderable(&self, registry: &BlockRegistry) -> bool {
        registry
            .get(self.block_id)
            .map(|def| def.is_renderable)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_pos_chunk_pos() {
        let pos = BlockPos::new(17, 64, -1);
        let chunk = pos.chunk_pos();
        assert_eq!(chunk, ChunkPos::new(1, -1));
    }

    #[test]
    fn block_pos_chunk_local() {
        let pos = BlockPos::new(17, 64, -1);
        let local = pos.chunk_local();
        assert_eq!(local, BlockPos::new(1, 64, 15));
    }

    #[test]
    fn transform_to_block_pos() {
        let transform = Transform::from_xyz(1.5, 64.0, -2.3);
        let block_pos: BlockPos = (&transform).into();
        assert_eq!(block_pos, BlockPos::new(1, 64, -3));
    }
}
