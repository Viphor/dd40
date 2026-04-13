//! Block placement logic for the player.
//!
//! This module reads the [`TargetedBlock`] resource every frame and, when the
//! player presses the place-block button (right mouse button), attempts to
//! place the player's currently held block into the face-adjacent voxel.
//!
//! # Flow
//!
//! 1. Read [`TargetedBlock`] — if no block is targeted, do nothing.
//! 2. Compute the placement position: `targeted.pos + targeted.face.normal()`.
//! 3. Look up the block currently occupying that position in [`ChunkCache`].
//! 4. Check [`Block::is_replaceable`] against the [`BlockRegistry`] — if the
//!    destination voxel is not replaceable (e.g. it already contains stone),
//!    do nothing.
//! 5. Write a [`PlaceBlockRequest`] message so the network layer forwards the
//!    request to the authoritative server.
//!
//! # Authoritativeness
//!
//! This system does **not** mutate [`ChunkCache`] directly.  The server is
//! authoritative: when it accepts the request it updates its own cache and
//! broadcasts a [`BlockPlaced`] message back to all clients.  The client-side
//! network layer receives that message and applies it to the local cache,
//! which then triggers a re-render.  This keeps the client and server caches
//! consistent without client-side prediction for placement.
//!
//! # Held block
//!
//! The block type the player currently intends to place is stored in
//! [`HeldBlock`].  Other systems (hotbar, inventory) should mutate this
//! resource to change what the player places.

use bevy::prelude::*;
use dd40_core::block::events::{BlockPlaced, PlaceBlockRequest};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::prelude::*;

use crate::block_interaction::targeting::TargetedBlock;

// ── Held-block resource ───────────────────────────────────────────────────────

/// The block type that the player will place on the next right-click.
///
/// Mutate this resource from a hotbar or inventory system to let the player
/// choose which block to place.  Defaults to [`BlockId::AIR`], which means no
/// block will be placed until something sensible is set.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_core::prelude::BlockId;
/// use dd40_player::block_interaction::HeldBlock;
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
        // Default to stone (id 1) as a sensible starting block.
        // Callers may override this via a hotbar or inventory system.
        Self {
            block_id: BlockId(1),
        }
    }
}

// ── Placement system ──────────────────────────────────────────────────────────

/// Reads player input and the current [`TargetedBlock`], then emits a
/// [`PlaceBlockRequest`] when the place-block button is pressed and the
/// destination voxel is replaceable.
///
/// # When does placement fire?
///
/// - Right mouse button is **just pressed** (single press, not held).
/// - [`TargetedBlock::pos`] and [`TargetedBlock::face`] are both `Some`.
/// - The voxel at the placement position (hit pos + face normal) is loaded and
///   [`Block::is_replaceable`] returns `true`.
/// - The [`HeldBlock`] is not [`BlockId::AIR`] — placing air is a no-op.
///
/// # No local mutation
///
/// This system intentionally does not write to [`ChunkCache`].  The server
/// applies the change and broadcasts [`BlockPlaced`] back; the client network
/// layer applies that to the local cache.
pub(super) fn try_place_block(
    mouse: Res<ButtonInput<MouseButton>>,
    targeted: Res<TargetedBlock>,
    held: Res<HeldBlock>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut requests: MessageWriter<PlaceBlockRequest>,
) {
    // Only fire on a fresh press so the player places one block per click.
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    // Nothing targeted → nothing to place against.
    let (Some(hit_pos), Some(face)) = (targeted.pos, targeted.face) else {
        return;
    };

    // Placing air is meaningless.
    if held.block_id == BlockId::AIR {
        return;
    }

    // Compute the world position of the voxel the new block will occupy.
    let normal = face.normal();
    let place_pos = BlockPos::new(
        hit_pos.x + normal.x,
        hit_pos.y + normal.y,
        hit_pos.z + normal.z,
    );

    // Look up what is currently at the placement position.
    let chunk_pos = place_pos.chunk_pos();
    let local = place_pos.chunk_local();

    // If the chunk is not loaded we cannot validate replaceability — skip.
    let Some(chunk) = cache.get(&chunk_pos) else {
        debug!("Placement skipped: chunk at {} is not loaded", chunk_pos);
        return;
    };

    // Guard against negative Y (chunk_local passes Y through unchanged).
    if local.y < 0 {
        return;
    }

    // If the voxel data is missing (out-of-bounds) also skip.
    let Some(existing) = chunk.get(local.x as usize, local.y as usize, local.z as usize) else {
        return;
    };

    // Only replace voxels that are explicitly marked as replaceable (e.g. air).
    if !existing.is_replaceable(&registry) {
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
/// This system is the **client-side apply step**.  The server broadcasts
/// [`BlockPlaced`] after it has validated and applied a [`PlaceBlockRequest`].
/// Any client (including the one that sent the request) receives the message
/// here and updates its own cache, which then triggers a mesh rebuild.
///
/// Writing the message here also lets other local systems (audio, particles,
/// debug UI) react to confirmed placements without querying the cache.
pub(super) fn apply_placed_blocks(
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
            // Chunk not loaded on this client — the data will be correct when
            // the chunk eventually loads from the server.
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
