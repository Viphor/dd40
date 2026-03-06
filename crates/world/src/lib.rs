use bevy::prelude::*;
use dd40_core::{
    Block, BlockId, BlockPos, BlockRegistry, ChunkPos, VanillaBlocks, WorldGenerationSet,
    CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z,
};

pub mod rendering;
pub use rendering::BlockRenderingPlugin;

/// Stores all blocks for a 16 × 256 × 16 area of the world.
///
/// Blocks are indexed as `blocks[x][y][z]`.
#[derive(Component)]
pub struct Chunk {
    pub pos: ChunkPos,
    blocks: Box<[[[Block; CHUNK_SIZE_Z]; CHUNK_SIZE_Y]; CHUNK_SIZE_X]>,
}

impl Chunk {
    /// Creates a new chunk filled with Air blocks.
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            blocks: Box::new(
                [[[Block::new(BlockId::AIR); CHUNK_SIZE_Z]; CHUNK_SIZE_Y]; CHUNK_SIZE_X],
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
                chunk.set_block(x, y, z, Block::new(VanillaBlocks::STONE));
            }
            for y in 58..62 {
                chunk.set_block(x, y, z, Block::new(VanillaBlocks::DIRT));
            }
            chunk.set_block(x, 62, z, Block::new(VanillaBlocks::GRASS));
        }
    }
    chunk
}

fn spawn_initial_chunks(mut commands: Commands, registry: Res<BlockRegistry>) {
    info!("Spawning initial chunks");
    for cx in -1..=1_i32 {
        for cz in -1..=1_i32 {
            let chunk_pos = ChunkPos::new(cx, cz);
            let chunk = generate_flat_chunk(chunk_pos);

            // Spawn individual block entities with rendering components
            for x in 0..CHUNK_SIZE_X {
                for y in 0..CHUNK_SIZE_Y {
                    for z in 0..CHUNK_SIZE_Z {
                        let block = chunk.get_block(x, y, z);
                        //info!(
                        //    "Trying to spawn block({}) at ({}, {}, {})",
                        //    registry
                        //        .get(block.block_id)
                        //        .map(|def| def.name.clone())
                        //        .unwrap_or("unknown".into()),
                        //    x,
                        //    y,
                        //    z
                        //);

                        // Only spawn entities for renderable blocks
                        if block.is_renderable(&registry) {
                            let global_x = chunk_pos.x * CHUNK_SIZE_X as i32 + x as i32;
                            let global_y = y as i32;
                            let global_z = chunk_pos.z * CHUNK_SIZE_Z as i32 + z as i32;

                            //info!(
                            //    "Spawning block at ({}, {}, {})",
                            //    global_x, global_y, global_z
                            //);
                            commands.spawn((block, BlockPos::new(global_x, global_y, global_z)));
                        }
                    }
                }
            }

            // Spawn the chunk entity after reading all blocks
            commands.spawn(chunk);
        }
    }
}

/// Helper function to spawn a block entity with all necessary components.
/// This will automatically get rendering components added by the BlockRenderingPlugin.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_core::{BlockId, BlockPos, VanillaBlocks};
/// use dd40_world::spawn_block;
///
/// fn my_system(mut commands: Commands) {
///     spawn_block(&mut commands, VanillaBlocks::STONE, BlockPos::new(10, 64, 10));
/// }
/// ```
pub fn spawn_block(commands: &mut Commands, block_id: BlockId, pos: BlockPos) -> Entity {
    commands.spawn((Block::new(block_id), pos)).id()
}

/// Helper function to spawn multiple blocks at once.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_core::{BlockId, BlockPos, VanillaBlocks};
/// use dd40_world::spawn_blocks;
///
/// fn my_system(mut commands: Commands) {
///     let blocks = vec![
///         (VanillaBlocks::STONE, BlockPos::new(0, 0, 0)),
///         (VanillaBlocks::GRASS, BlockPos::new(1, 0, 0)),
///         (VanillaBlocks::DIRT, BlockPos::new(2, 0, 0)),
///     ];
///     spawn_blocks(&mut commands, &blocks);
/// }
/// ```
pub fn spawn_blocks(commands: &mut Commands, blocks: &[(BlockId, BlockPos)]) -> Vec<Entity> {
    blocks
        .iter()
        .map(|(block_id, pos)| spawn_block(commands, *block_id, *pos))
        .collect()
}

/// Bevy plugin that spawns an initial set of world chunks on startup.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BlockRenderingPlugin)
            .add_systems(Startup, spawn_initial_chunks.in_set(WorldGenerationSet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_chunk_terrain() {
        let chunk = generate_flat_chunk(ChunkPos::new(0, 0));
        assert_eq!(chunk.get_block(0, 0, 0).block_id, VanillaBlocks::STONE);
        assert_eq!(chunk.get_block(0, 62, 0).block_id, VanillaBlocks::GRASS);
        assert_eq!(chunk.get_block(0, 63, 0).block_id, BlockId::AIR);
    }

    #[test]
    fn chunk_get_set_block() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.set_block(1, 2, 3, Block::new(VanillaBlocks::SAND));
        assert_eq!(chunk.get_block(1, 2, 3).block_id, VanillaBlocks::SAND);
    }
}
