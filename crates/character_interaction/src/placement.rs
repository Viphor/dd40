//! Block placement logic for any [`Character`] entity.
//!
//! This module reads each character's [`TargetedBlock`] every frame and, when
//! the player presses the place-block button (right mouse button), attempts to
//! place the block declared by the character's [`ActiveItem`] into the
//! face-adjacent voxel.
//!
//! # Flow
//!
//! 1. Read the character's [`TargetedBlock`] — if no block is targeted, do nothing.
//! 2. Read the character's [`ActiveItem`] — if `None` or the item has no
//!    [`placeable`][ItemDefinition::placeable] block, do nothing.
//! 3. Compute the placement position: `targeted.pos + targeted.face.normal()`.
//! 4. Look up the block currently occupying that position in [`ChunkCache`].
//! 5. Check [`BlockRegistry::is_replaceable`] — if the destination voxel is
//!    not replaceable (e.g. it already contains stone), do nothing.
//! 6. Write a [`PlaceBlockRequest`] message so the network layer forwards the
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
//! # Source of the placement block
//!
//! The block to place comes from the character's [`ActiveItem`] via
//! [`ItemRegistry`]. Inventory crates write [`ActiveItem`]; this system never
//! reads any inventory layout directly. A character with no [`ActiveItem`]
//! component, with `ActiveItem(None)`, or whose item has no `placeable`
//! field is treated as holding nothing placeable — right-click is a no-op.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::block::events::{BlockPlaced, PlaceBlockRequest};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::prelude::*;
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::{ItemDefinition, ItemRegistry};

/// Reads input and the local character's [`TargetedBlock`] + [`ActiveItem`],
/// then emits a [`PlaceBlockRequest`] when the place-block button is pressed
/// and the destination voxel is replaceable.
///
/// # When does placement fire?
///
/// - Right mouse button is **just pressed** (single press, not held).
/// - The character has a [`TargetedBlock`] with both `pos` and `face` set.
/// - The character has an [`ActiveItem`] whose
///   [`ItemDefinition::placeable`] is `Some(block_id)` and `block_id` is not
///   [`BlockId::AIR`].
/// - The voxel at the placement position is loaded and
///   [`BlockRegistry::is_replaceable`] returns `true`.
///
/// # No local mutation
///
/// This system intentionally does not write to [`ChunkCache`]. The server
/// applies the change and broadcasts [`BlockPlaced`] back; the client network
/// layer applies that to the local cache.
pub(crate) fn try_place_block(
    mouse: Res<ButtonInput<MouseButton>>,
    character_query: Query<(&TargetedBlock, Option<&ActiveItem>), With<Character>>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    items: Res<ItemRegistry>,
    mut requests: MessageWriter<PlaceBlockRequest>,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let Some((targeted, active)) = character_query.iter().next() else { return };
    let (Some(hit_pos), Some(face)) = (targeted.pos, targeted.face) else {
        return;
    };

    let Some(block_id) = placeable_block(active, &items) else { return };
    if block_id == BlockId::AIR {
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
        block_id, place_pos, face, hit_pos
    );

    requests.write(PlaceBlockRequest {
        pos: place_pos,
        block_id,
    });
}

/// Resolves the [`BlockId`] a character is set to place by following
/// `ActiveItem -> ItemRegistry -> ItemDefinition::placeable`.
///
/// Returns `None` if the character is holding nothing or the held item has no
/// `placeable` field.
fn placeable_block(active: Option<&ActiveItem>, items: &ItemRegistry) -> Option<BlockId> {
    let stack = active?.0?;
    let def: &ItemDefinition = items.get(stack.item)?;
    def.placeable
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
