use bevy::prelude::*;
use dd40_core::{Block, BlockType, ChunkPos, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z};

/// Stores all blocks for a 16 × 256 × 16 area of the world.
///
/// Blocks are indexed as `blocks[x][y][z]`.
#[derive(Component)]
pub struct Chunk {
    pub pos: ChunkPos,
    blocks: Box<[[[Block; CHUNK_SIZE_Z]; CHUNK_SIZE_Y]; CHUNK_SIZE_X]>,
}

impl Chunk {
    /// Creates a new chunk filled with [`BlockType::Air`].
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            blocks: Box::new(
                [[[Block::new(BlockType::Air); CHUNK_SIZE_Z]; CHUNK_SIZE_Y]; CHUNK_SIZE_X],
            ),
        }
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> Block {
        self.blocks[x][y][z]
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: Block) {
        self.blocks[x][y][z] = block;
    }
}

/// Generates a flat-terrain chunk:
/// * y = 0       : Stone (acts as bedrock)
/// * y = 1–57    : Stone
/// * y = 58–61   : Dirt
/// * y = 62      : Grass
/// * y = 63–255  : Air
pub fn generate_flat_chunk(pos: ChunkPos) -> Chunk {
    let mut chunk = Chunk::new(pos);
    for x in 0..CHUNK_SIZE_X {
        for z in 0..CHUNK_SIZE_Z {
            for y in 0..58 {
                chunk.set_block(x, y, z, Block::new(BlockType::Stone));
            }
            for y in 58..62 {
                chunk.set_block(x, y, z, Block::new(BlockType::Dirt));
            }
            chunk.set_block(x, 62, z, Block::new(BlockType::Grass));
        }
    }
    chunk
}

fn spawn_initial_chunks(mut commands: Commands) {
    for cx in -1..=1_i32 {
        for cz in -1..=1_i32 {
            let pos = ChunkPos::new(cx, cz);
            commands.spawn(generate_flat_chunk(pos));
        }
    }
}

/// Bevy plugin that spawns an initial set of world chunks on startup.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_initial_chunks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_chunk_terrain() {
        let chunk = generate_flat_chunk(ChunkPos::new(0, 0));
        assert_eq!(chunk.get_block(0, 0, 0).block_type, BlockType::Stone);
        assert_eq!(chunk.get_block(0, 62, 0).block_type, BlockType::Grass);
        assert_eq!(chunk.get_block(0, 63, 0).block_type, BlockType::Air);
    }

    #[test]
    fn chunk_get_set_block() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.set_block(1, 2, 3, Block::new(BlockType::Sand));
        assert_eq!(chunk.get_block(1, 2, 3).block_type, BlockType::Sand);
    }
}
