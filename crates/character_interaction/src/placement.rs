//! Block placement logic for any [`Character`] entity.
//!
//! This module reads each character's [`TargetedBlock`] every frame and, when
//! the character's [`CharacterInput::place`] flag is `true`, attempts to
//! place the block declared by the character's [`ActiveItem`] into the
//! face-adjacent voxel.
//!
//! ## Input source
//!
//! Placement is driven exclusively by [`CharacterInput::place`]. The local
//! player's input layer (`dd40_player_movement`) is responsible for
//! translating mouse/keyboard into that flag — this module is input-device
//! agnostic and runs identically on the client and the server.
//!
//! ## Reset semantics ("decision D")
//!
//! `CharacterInput::place` is treated as a one-shot intent: every tick where
//! `place == true` consumes it, regardless of whether the placement
//! actually succeeded. This mirrors how `CharacterInput::jump` is consumed
//! by the controller and prevents a single right-click from queuing up
//! multiple identical attempts on subsequent ticks.
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
//! field is treated as holding nothing placeable — `place` is consumed but
//! no message is emitted.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::block::events::{BlockPlaced, PlaceBlockRequest};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::prelude::*;
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::{ItemDefinition, ItemRegistry};

/// Pure placement-step result returned by [`step_placement`].
#[derive(Debug, Default, Clone)]
pub(crate) struct PlacementStep {
    /// `Some(req)` if a [`PlaceBlockRequest`] should be emitted.
    pub request: Option<PlaceBlockRequest>,
}

/// Pure placement state-machine step.
///
/// `destination` returns:
/// - `None` if the destination voxel cannot be evaluated (chunk unloaded,
///   below world).
/// - `Some(true)` if the destination is replaceable.
/// - `Some(false)` if the destination is occupied by a non-replaceable block.
pub(crate) fn step_placement(
    place_intent: bool,
    targeted: &TargetedBlock,
    placeable: Option<BlockId>,
    destination: impl FnOnce(BlockPos) -> Option<bool>,
) -> PlacementStep {
    if !place_intent {
        return PlacementStep::default();
    }
    let (Some(hit_pos), Some(face)) = (targeted.pos, targeted.face) else {
        return PlacementStep::default();
    };
    let Some(block_id) = placeable else {
        return PlacementStep::default();
    };
    if block_id == BlockId::AIR {
        return PlacementStep::default();
    }
    let normal = face.normal();
    let place_pos = BlockPos::new(
        hit_pos.x + normal.x,
        hit_pos.y + normal.y,
        hit_pos.z + normal.z,
    );
    match destination(place_pos) {
        Some(true) => PlacementStep {
            request: Some(PlaceBlockRequest {
                pos: place_pos,
                block_id,
            }),
        },
        _ => PlacementStep::default(),
    }
}

/// Per-character placement system.
///
/// Reads `(&mut CharacterInput, &TargetedBlock, Option<&ActiveItem>)` for
/// every [`Character`], runs [`step_placement`], emits a
/// [`PlaceBlockRequest`] when appropriate, and **always resets**
/// `CharacterInput::place` to `false` after one tick where it was `true`
/// (see "decision D" in the module docs).
pub(crate) fn try_place_block(
    mut character_query: Query<
        (&mut CharacterInput, &TargetedBlock, Option<&ActiveItem>),
        With<Character>,
    >,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    items: Res<ItemRegistry>,
    mut requests: MessageWriter<PlaceBlockRequest>,
) {
    for (mut input, targeted, active) in &mut character_query {
        if !input.place {
            continue;
        }

        let placeable = placeable_block(active, &items);

        let destination = |place_pos: BlockPos| -> Option<bool> {
            let chunk_pos = place_pos.chunk_pos();
            let local = place_pos.chunk_local();
            if local.y < 0 {
                return None;
            }
            let chunk = cache.get(&chunk_pos)?;
            let existing =
                chunk.get(local.x as usize, local.y as usize, local.z as usize)?;
            Some(registry.is_replaceable(&existing))
        };

        let step = step_placement(input.place, targeted, placeable, destination);

        if let Some(req) = step.request {
            debug!(
                "Requesting placement of {:?} at {}",
                req.block_id, req.pos
            );
            requests.write(req);
        }

        input.place = false;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_character_core::targeted_block::BlockFace;

    fn target_at(pos: BlockPos, face: BlockFace) -> TargetedBlock {
        TargetedBlock {
            pos: Some(pos),
            face: Some(face),
            block_id: Some(BlockId(1)),
        }
    }

    #[test]
    fn place_false_emits_nothing() {
        let s = step_placement(
            false,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId(5)),
            |_| Some(true),
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_no_target_emits_nothing() {
        let s = step_placement(
            true,
            &TargetedBlock::default(),
            Some(BlockId(5)),
            |_| Some(true),
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_no_placeable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            None,
            |_| Some(true),
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_air_placeable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId::AIR),
            |_| Some(true),
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_destination_not_replaceable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId(5)),
            |_| Some(false),
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_destination_unloaded_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId(5)),
            |_| None,
        );
        assert!(s.request.is_none());
    }

    #[test]
    fn place_true_replaceable_destination_emits_request_at_face_normal() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(3, 64, 5), BlockFace::Top),
            Some(BlockId(7)),
            |_| Some(true),
        );
        let req = s.request.expect("expected a placement request");
        assert_eq!(req.block_id, BlockId(7));
        assert_eq!(req.pos, BlockPos::new(3, 65, 5));
    }
}
