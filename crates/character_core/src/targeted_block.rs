//! Per-character "what block am I looking at" state.
//!
//! [`TargetedBlock`] is updated each frame by the raycast system in
//! `dd40_character_interaction` and read by mining, placement, HUDs, and
//! optional behaviour crates such as `dd40_auto_tool_swap`.  Lives in
//! `dd40_character_core` so any Tier-1 crate can read it without taking a
//! dependency on the interaction crate.

use bevy::prelude::*;
use dd40_core::prelude::{BlockId, BlockPos};

/// The face of a block that a ray entered from.
///
/// Used to decide where a placed block goes (caller adds [`BlockFace::normal`]
/// to the hit position).
///
/// # Placement offset
///
/// ```
/// use dd40_character_core::targeted_block::BlockFace;
/// use dd40_core::prelude::BlockPos;
///
/// let hit_pos = BlockPos::new(3, 64, 5);
/// let face    = BlockFace::Top;
/// let place_pos = BlockPos::new(
///     hit_pos.x + face.normal().x,
///     hit_pos.y + face.normal().y,
///     hit_pos.z + face.normal().z,
/// );
/// assert_eq!(place_pos, BlockPos::new(3, 65, 5));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum BlockFace {
    /// The +Y face (ray came from above).
    Top,
    /// The -Y face (ray came from below).
    Bottom,
    /// The +X face.
    East,
    /// The -X face.
    West,
    /// The +Z face.
    South,
    /// The -Z face.
    North,
}

impl BlockFace {
    /// Returns the unit offset to add to the hit block's [`BlockPos`] to get
    /// the face-adjacent voxel (where a new block would be placed).
    pub fn normal(self) -> BlockPos {
        match self {
            BlockFace::Top => BlockPos::new(0, 1, 0),
            BlockFace::Bottom => BlockPos::new(0, -1, 0),
            BlockFace::East => BlockPos::new(1, 0, 0),
            BlockFace::West => BlockPos::new(-1, 0, 0),
            BlockFace::South => BlockPos::new(0, 0, 1),
            BlockFace::North => BlockPos::new(0, 0, -1),
        }
    }
}

/// The block a character is currently looking at, if any.
///
/// Attach to any [`Character`][crate::components::Character] entity.
/// `dd40_character_interaction`'s targeting system writes to this component
/// each frame; mining, placement, HUDs, and selector plugins (such as
/// `dd40_auto_tool_swap`) read it.
#[derive(Component, Debug, Default, Clone, Reflect)]
#[reflect(Component)]
pub struct TargetedBlock {
    /// World position of the targeted block, or `None` if no block is in range.
    pub pos: Option<BlockPos>,
    /// Face of the targeted block the ray entered from.
    pub face: Option<BlockFace>,
    /// Block id of the targeted block.
    pub block_id: Option<BlockId>,
}
