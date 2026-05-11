use bevy::{ecs::component::Component, math::Vec3, reflect::Reflect, transform::components::Transform};
use serde::{Deserialize, Serialize};

pub mod registry;
//pub mod storage;

pub use registry::{BlockDefinition, BlockRegistry};

use crate::chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Z, ChunkPos};

// ---------------------------------------------------------------------------
// Collision shape
// ---------------------------------------------------------------------------

/// Collision shape for a block type.
///
/// All variants must stay **within** the 1×1×1 block cell.  The physics
/// solver reads this directly from [`BlockDefinition::collision_shape`] via
/// [`BlockRegistry`] — it is part of the block definition rather than a
/// separate resource.
///
/// This is the extensibility point for stairs, slabs, lecterns, etc.
///
/// # Example
///
/// ```
/// use bevy::math::Vec3;
/// use dd40_core::block::CollisionShape;
/// let slab = CollisionShape::Box { min: Vec3::ZERO, max: Vec3::new(1.0, 0.5, 1.0) };
/// ```
///
/// [`BlockDefinition::collision_shape`]: crate::block::registry::BlockDefinition::collision_shape
/// [`BlockRegistry`]: crate::block::registry::BlockRegistry
#[derive(Debug, Clone, Reflect)]
pub enum CollisionShape {
    /// Solid unit cube — the default for all opaque blocks.
    FullCube,
    /// No collision at all (air, torches, etc.).
    None,
    /// An AABB within the cell, specified as min/max in **cell-local** space
    /// (i.e. each component in `[0, 1]`).  The cell origin is the
    /// block's minimum corner.
    Box {
        /// Minimum corner in cell-local coordinates (`[0, 1]` range).
        min: Vec3,
        /// Maximum corner in cell-local coordinates (`[0, 1]` range).
        max: Vec3,
    },
}

impl Default for CollisionShape {
    fn default() -> Self {
        Self::FullCube
    }
}

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_is_replaceable() {
        use bevy::app::App;
        use bevy::color::Color;
        use bevy::prelude::{Commands, ResMut};
        use registry::BlockRegistry;

        let mut app = App::new();
        app.insert_resource(BlockRegistry::new());

        // Register stone (BlockId(1)) without the replaceable flag so it defaults to false
        app.add_systems(
            bevy::app::Startup,
            |mut registry: ResMut<BlockRegistry>, mut commands: Commands| {
                registry.register(
                    BlockDefinition::new(BlockId(1), "stone")
                        .with_color(Color::srgb(0.5, 0.5, 0.5)),
                    &mut commands,
                );
            },
        );

        app.update();

        let registry = app.world().resource::<BlockRegistry>();

        let air = Block::new(BlockId::AIR);
        let stone = Block::new(BlockId(1));

        assert!(registry.is_replaceable(&air), "air should be replaceable");
        assert!(
            !registry.is_replaceable(&stone),
            "stone should not be replaceable"
        );
    }

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
