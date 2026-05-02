//! Block placement logic for any [`Character`] entity.
//!
//! This module reads the [`TargetedBlock`] resource every frame and, when the
//! player presses the place-block button (right mouse button), attempts to
//! place the currently held block into the face-adjacent voxel.
//!
//! # Flow
//!
//! 1. Read [`TargetedBlock`] — if no block is targeted, do nothing.
//! 2. Compute the placement position: `targeted.pos + targeted.face.normal()`.
//! 3. Look up the block currently occupying that position in [`ChunkCache`].
//! 4. Check [`BlockRegistry::is_replaceable`] — if the destination voxel is
//!    not replaceable (e.g. it already contains stone), do nothing.
//! 5. Write a [`PlaceBlockRequest`] message so the network layer forwards the
//!    request to the authoritative server.
//!
//! # Authoritativeness
//!
//! This system does **not** mutate [`ChunkCache`] directly. The server is
//! authoritative: when it accepts the request it updates its own cache and
//! broadcasts a [`BlockPlaced`] message back to all clients. The client-side
//! network layer receives that message and applies it to the local cache,
//! which then triggers a re-render. This keeps the client and server caches
//! consistent without client-side prediction for placement.
//!
//! # Held block
//!
//! The block type currently intended for placement is stored in [`HeldBlock`].
//! Other systems (hotbar, inventory) should mutate this resource to change
//! what gets placed.

use bevy::prelude::*;
use dd40_core::block::events::{BlockPlaced, PlaceBlockRequest};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::prelude::*;

use crate::targeting::TargetedBlock;

// ── Held-block resource ───────────────────────────────────────────────────────

/// The block type that will be placed on the next right-click.
///
/// Mutate this resource from a hotbar or inventory system to let the player
/// choose which block to place. Defaults to [`BlockId`] `1` (stone).
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_core::prelude::BlockId;
/// use dd40_character_interaction::HeldBlock;
///
/// fn select_stone(mut held: ResMut<HeldBlock>) {
///     held.block_id = BlockId(1); // stone
/// }
/// ```
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct HeldBlock {
    /// The [`BlockId`] that will be placed on the next placement action.
    pub block_id: BlockId,
}

impl Default for HeldBlock {
    fn default() -> Self {
        Self {
            block_id: BlockId(1),
        }
    }
}

// ── Placement system ──────────────────────────────────────────────────────────

/// Reads input and the current [`TargetedBlock`], then emits a
/// [`PlaceBlockRequest`] when the place-block button is pressed and the
/// destination voxel is replaceable.
///
/// # When does placement fire?
///
/// - Right mouse button is **just pressed** (single press, not held).
/// - [`TargetedBlock::pos`] and [`TargetedBlock::face`] are both `Some`.
/// - The voxel at the placement position is loaded and
///   [`BlockRegistry::is_replaceable`] returns `true`.
/// - The [`HeldBlock`] is not [`BlockId::AIR`] — placing air is a no-op.
///
/// # No local mutation
///
/// This system intentionally does not write to [`ChunkCache`]. The server
/// applies the change and broadcasts [`BlockPlaced`] back; the client network
/// layer applies that to the local cache.
pub(crate) fn try_place_block(
    mouse: Res<ButtonInput<MouseButton>>,
    targeted: Res<TargetedBlock>,
    held: Res<HeldBlock>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut requests: MessageWriter<PlaceBlockRequest>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let (Some(hit_pos), Some(face)) = (targeted.pos, targeted.face) else {
        return;
    };

    if held.block_id == BlockId::AIR {
        return;
    }

    let normal = face.normal();
    let place_pos = BlockPos::new(
        hit_pos.x + normal.x,
        hit_pos.y + normal.y,
        hit_pos.z + normal.z,
    );

    let chunk_pos = place_pos.chunk_pos();
    let local = place_pos.chunk_local();

    let Some(chunk) = cache.get(&chunk_pos) else {
        debug!("Placement skipped: chunk at {} is not loaded", chunk_pos);
        return;
    };

    if local.y < 0 {
        return;
    }

    let Some(existing) = chunk.get(local.x as usize, local.y as usize, local.z as usize) else {
        return;
    };

    if !registry.is_replaceable(&existing) {
        debug!(
            "Placement blocked: voxel at {} (block {:?}) is not replaceable",
            place_pos, existing.block_id
        );
        return;
    }

    debug!(
        "Requesting placement of {:?} at {} (face {:?} of {})",
        held.block_id, place_pos, face, hit_pos
    );

    requests.write(PlaceBlockRequest {
        pos: place_pos,
        block_id: held.block_id,
    });
}

/// Listens for confirmed [`BlockPlaced`] messages and applies them to the
/// local [`ChunkCache`].
///
/// The server broadcasts [`BlockPlaced`] after it has validated and applied a
/// [`PlaceBlockRequest`]. Any client receives the message here and updates its
/// own cache, which then triggers a mesh rebuild.
pub(crate) fn apply_placed_blocks(
    mut reader: MessageReader<BlockPlaced>,
    mut cache: ResMut<ChunkCache>,
) {
    for placed in reader.read() {
        let chunk_pos = placed.pos.chunk_pos();
        let local = placed.pos.chunk_local();

        if local.y < 0 {
            continue;
        }

        let Some(chunk) = cache.get_mut(&chunk_pos) else {
            continue;
        };

        chunk.set(
            local.x as usize,
            local.y as usize,
            local.z as usize,
            dd40_core::block::Block::new(placed.block_id),
        );

        debug!(
            "Applied confirmed placement of {:?} at {}",
            placed.block_id, placed.pos
        );
    }
}
