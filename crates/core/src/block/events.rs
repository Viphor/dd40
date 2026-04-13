use bevy::ecs::{entity::Entity, message::Message};
use serde::{Deserialize, Serialize};

use crate::block::{BlockId, BlockPos};

/// Sent by a player system to request placing a block at a given world position.
///
/// This message travels from the local placement system to the network layer
/// (and ultimately the server), which validates the request — checking that the
/// target voxel [`is_replaceable`] — before applying the change and
/// broadcasting a [`BlockPlaced`] message to all connected clients that have
/// the affected chunk loaded.
///
/// # Authoritativeness
///
/// Writing this message does **not** immediately mutate the local
/// [`ChunkCache`].  The server is authoritative: the local cache is updated
/// only when the corresponding [`BlockPlaced`] message is received back from
/// the server (or applied locally on a listen-server).
///
/// [`is_replaceable`]: crate::block::BlockDefinition::is_replaceable
/// [`ChunkCache`]: crate::chunk::cache::ChunkCache
#[derive(Message, Debug, Clone, Serialize, Deserialize)]
pub struct PlaceBlockRequest {
    /// World-space position of the voxel to place the block in.
    pub pos: BlockPos,
    /// The block type to place.
    pub block_id: BlockId,
}

/// Fired when a block has been confirmed placed — either by the authoritative
/// server broadcasting the change back, or by a local listen-server applying a
/// [`PlaceBlockRequest`] directly.
///
/// The [`ChunkCache`] is updated **before** this message is written, so
/// listeners can immediately query the new block data.  Rendering and audio
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
