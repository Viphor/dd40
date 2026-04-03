use std::ops::Range;

use bevy::ecs::resource::Resource;
use dd40_core::{prelude::*, vanilla_blocks::VanillaBlocks};

use crate::generators::WorldGenerator;

#[derive(Clone)]
pub struct Layer {
    pub block_id: BlockId,
    pub height_range: Range<usize>,
}

#[derive(Resource, Clone)]
pub struct FlatWorldGenerator(pub Vec<Layer>);

impl Default for FlatWorldGenerator {
    fn default() -> Self {
        Self(vec![
            Layer {
                block_id: VanillaBlocks::STONE.into(),
                height_range: 0..58,
            },
            Layer {
                block_id: VanillaBlocks::DIRT.into(),
                height_range: 58..62,
            },
            Layer {
                block_id: VanillaBlocks::GRASS.into(),
                height_range: 62..63,
            },
        ])
    }
}

impl WorldGenerator for FlatWorldGenerator {
    fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        for x in 0..CHUNK_SIZE_X {
            for z in 0..CHUNK_SIZE_Z {
                for layer in &self.0 {
                    for y in layer.height_range.clone() {
                        chunk.set(x, y, z, Block::new(layer.block_id));
                    }
                }
            }
        }
        chunk
    }
}
