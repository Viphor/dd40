use bevy::ecs::{entity::Entity, message::Message};
use serde::{Deserialize, Serialize};

use crate::block::{BlockId, BlockPos};

/// Event fired when a block is placed by a player or system.
/// This is distinct from world generation - it represents an intentional placement action.
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockPlaced {
    pub pos: BlockPos,
    pub block_id: BlockId,
    /// Optional entity that placed the block (e.g., player entity)
    #[serde(skip)]
    pub placer: Option<Entity>,
}

/// Event fired when a block is removed/broken by a player or system.
/// This is distinct from world generation - it represents an intentional removal action.
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockRemoved {
    pub pos: BlockPos,
    pub previous_block_id: BlockId,
    /// Optional entity that removed the block (e.g., player entity)
    #[serde(skip)]
    pub remover: Option<Entity>,
}

/// Event fired when a block changes type (e.g., water freezing to ice).
/// This is for transformations rather than placement/removal.
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct BlockChanged {
    pub pos: BlockPos,
    pub old_block_id: BlockId,
    pub new_block_id: BlockId,
}
