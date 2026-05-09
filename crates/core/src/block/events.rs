use bevy::ecs::{entity::Entity, message::Message};
use serde::{Deserialize, Serialize};

use crate::block::{BlockId, BlockPos};

/// Fired when a block has been placed.
///
/// The [`ChunkCache`] is updated **before** this message is written, so
/// listeners can immediately query the new block data. Rendering and audio
/// systems should listen for this message to trigger mesh rebuilds and sound
/// effects.
///
/// [`ChunkCache`]: crate::chunk::cache::ChunkCache
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlaced {
    /// World-space position of the placed block.
    pub pos: BlockPos,
    /// The block type that was placed.
    pub block_id: BlockId,
    /// The entity that placed the block, if known (e.g. a player entity).
    #[serde(skip)]
    pub placer: Option<Entity>,
}

/// Fired when a block is removed/broken by a player or system.
///
/// This is distinct from world generation — it represents an intentional
/// removal action.
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockRemoved {
    /// World-space position of the removed block.
    pub pos: BlockPos,
    /// The block type that was in this position before removal.
    pub previous_block_id: BlockId,
    /// The entity that removed the block, if known.
    #[serde(skip)]
    pub remover: Option<Entity>,
}

/// Fired when a block changes type in place (e.g. water freezing to ice).
///
/// This represents a transformation rather than a placement or removal.
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockChanged {
    /// World-space position of the changed block.
    pub pos: BlockPos,
    /// The block type before the change.
    pub old_block_id: BlockId,
    /// The block type after the change.
    pub new_block_id: BlockId,
}
