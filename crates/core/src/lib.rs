use bevy::prelude::*;

/// Represents the type of a block in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum BlockType {
    Air,
    Stone,
    Dirt,
    Grass,
    Sand,
}

/// A single block, storing its type.
#[derive(Debug, Clone, Copy, Component, Reflect)]
pub struct Block {
    pub block_type: BlockType,
}

impl Block {
    pub fn new(block_type: BlockType) -> Self {
        Self { block_type }
    }

    pub fn is_solid(&self) -> bool {
        self.block_type != BlockType::Air
    }
}

/// Width (X) of a chunk in blocks.
pub const CHUNK_SIZE_X: usize = 16;
/// Height (Y) of a chunk in blocks.
pub const CHUNK_SIZE_Y: usize = 256;
/// Depth (Z) of a chunk in blocks.
pub const CHUNK_SIZE_Z: usize = 16;

/// Position of a chunk in chunk-space coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Global integer block position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Returns the chunk-space position that contains this block.
    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos {
            x: self.x.div_euclid(CHUNK_SIZE_X as i32),
            z: self.z.div_euclid(CHUNK_SIZE_Z as i32),
        }
    }
}

/// Bevy plugin that registers core types with the reflection system.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Block>()
            .register_type::<BlockType>()
            .register_type::<ChunkPos>()
            .register_type::<BlockPos>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_is_solid() {
        assert!(Block::new(BlockType::Stone).is_solid());
        assert!(!Block::new(BlockType::Air).is_solid());
    }

    #[test]
    fn block_pos_chunk_pos() {
        let pos = BlockPos::new(17, 64, -1);
        let chunk = pos.chunk_pos();
        assert_eq!(chunk, ChunkPos::new(1, -1));
    }
}
