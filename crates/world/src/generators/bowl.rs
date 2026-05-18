use std::ops::Range;

use bevy::ecs::resource::Resource;
use dd40_core::prelude::*;

use crate::generators::WorldGenerator;

/// A single horizontal layer in a flat world.
#[derive(Clone)]
pub struct Layer {
    /// The block that fills this layer.
    pub block_id: BlockId,
    /// The Y-coordinate range (exclusive) this layer occupies.
    pub height_range: Range<usize>,
}

/// World generator that produces a bowl world made of configurable
/// horizontal layers.
///
/// Construct an instance by supplying the desired [`Layer`] stack.  There is
/// no `Default` implementation — callers must explicitly choose which block IDs
/// fill each layer (typically using constants from `dd40_vanilla_palette`).
///
/// # Example
///
/// ```
/// use dd40_world::generators::bowl::{BowlWorldGenerator, Layer};
/// use dd40_core::block::BlockId;
///
/// // Using raw BlockId constants from your palette:
/// const STONE: BlockId = BlockId(1);
/// const DIRT:  BlockId = BlockId(2);
/// const GRASS: BlockId = BlockId(3);
///
/// let generator = BowlWorldGenerator(vec![
///     Layer { block_id: STONE, height_range: 0..58 },
///     Layer { block_id: DIRT,  height_range: 58..62 },
///     Layer { block_id: GRASS, height_range: 62..63 },
/// ]);
/// ```
#[derive(Resource, Clone)]
pub struct BowlWorldGenerator(pub Vec<Layer>);

impl WorldGenerator for BowlWorldGenerator {
    fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        for x in 0..CHUNK_SIZE_X {
            for z in 0..CHUNK_SIZE_Z {
                for (i, layer) in self.0.iter().enumerate() {
                    if i == self.0.len() - 1 {
                        let distance_from_spawn = pos.x.abs() + pos.z.abs();
                        let top_layer = layer.height_range.end;
                        // Last layer: fill to the top of the chunk.
                        for y in top_layer..top_layer + distance_from_spawn as usize {
                            if y >= CHUNK_SIZE_Y {
                                break;
                            }
                            chunk.set(x, y, z, Block::new(layer.block_id));
                        }
                    }
                    for y in layer.height_range.clone() {
                        chunk.set(x, y, z, Block::new(layer.block_id));
                    }
                }
            }
        }
        chunk
    }
}
