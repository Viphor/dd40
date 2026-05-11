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
//! player's input layer (`dd40_player_input`) is responsible for
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
//! # Authority model (versioned-chunk-cache pipeline)
//!
//! This system pushes a predicted [`ChunkChange::Place`] directly onto the
//! local [`ChunkCache`] on **both** the client and the server. Because
//! [`CharacterInput`] is replicated, the server runs the exact same
//! placement code against the same per-tick input — the client predicts
//! optimistically, the server confirms authoritatively via
//! [`ChunkAuthorityPlugin`](dd40_core::chunk::ChunkAuthorityPlugin).
//!
//! The placing player sees their placement persist immediately on screen
//! (the predicted change mutates `chunk.data` at push time). Other clients
//! will see the placement once `ChunkUpdate` broadcasting is wired (Phase 4
//! of the versioned-chunk-cache plan).
//!
//! Validation is performed by chunk-change validators on the server:
//! [`default_block_registry_validator`](dd40_core::chunk::default_block_registry_validator)
//! enforces replaceability;
//! [`character_collision_validator`](crate::validators::character_collision_validator)
//! enforces that no character occupies the target cell. Both run in
//! [`ChunkAuthoritySet::Validate`] before the commit pass.
//!
//! # Source of the placement block
//!
//! The block to place comes from the character's [`ActiveItem`] via
//! [`ItemRegistry`]. Inventory crates write [`ActiveItem`]; this system never
//! reads any inventory layout directly. A character with no [`ActiveItem`]
//! component, with `ActiveItem(None)`, or whose item has no `placeable`
//! field is treated as holding nothing placeable — `place` is consumed but
//! no change is queued.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::chunk::{ChunkChange, change::BlockLocal};
use dd40_core::prelude::*;
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::{ItemDefinition, ItemRegistry};

/// Outcome of one placement step.
///
/// Returned by [`step_placement`]. The world-space `pos` and `block_id`
/// are everything the caller needs to push a `ChunkChange::Place` onto
/// the local chunk cache — the conversion to chunk-local coordinates
/// happens in [`try_place_block`] (it requires access to the cache).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PlacementStep {
    /// `Some((world_pos, block_id))` if a placement should be enqueued.
    pub place: Option<(BlockPos, BlockId)>,
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
            place: Some((place_pos, block_id)),
        },
        _ => PlacementStep::default(),
    }
}

/// Per-character placement system.
///
/// Reads `(&mut CharacterInput, &TargetedBlock, Option<&ActiveItem>)` for
/// every [`Character`], runs [`step_placement`], pushes a predicted
/// [`ChunkChange::Place`] when appropriate, and **always resets**
/// `CharacterInput::place` to `false` after one tick where it was `true`
/// (see "decision D" in the module docs).
pub(crate) fn try_place_block(
    mut character_query: Query<
        (&mut CharacterInput, &TargetedBlock, Option<&ActiveItem>),
        With<Character>,
    >,
    mut cache: ResMut<ChunkCache>,
    registry: Res<BlockRegistry>,
    items: Res<ItemRegistry>,
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

        if let Some((place_pos, block_id)) = step.place {
            let chunk_pos = place_pos.chunk_pos();
            let local_world = place_pos.chunk_local();
            // We already validated `local.y >= 0` inside `destination`,
            // and the X/Z bounds are guaranteed by `chunk_local`'s
            // `rem_euclid`. Build the typed `BlockLocal` accordingly.
            let Some(local) = BlockLocal::try_new(
                local_world.x as u8,
                local_world.y as u16,
                local_world.z as u8,
            ) else {
                warn!(
                    "Refusing placement at {} — could not build a valid BlockLocal",
                    place_pos
                );
                input.place = false;
                continue;
            };
            debug!(
                "Predicting placement of {:?} at {} (chunk {} local {:?})",
                block_id, place_pos, chunk_pos, local
            );
            if !cache.push_predicted(chunk_pos, ChunkChange::new_place(local, block_id)) {
                debug!(
                    "Placement dropped — chunk {} not present in cache",
                    chunk_pos
                );
            }
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
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_no_target_emits_nothing() {
        let s = step_placement(
            true,
            &TargetedBlock::default(),
            Some(BlockId(5)),
            |_| Some(true),
        );
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_no_placeable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            None,
            |_| Some(true),
        );
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_air_placeable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId::AIR),
            |_| Some(true),
        );
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_destination_not_replaceable_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId(5)),
            |_| Some(false),
        );
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_destination_unloaded_emits_nothing() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(0, 0, 0), BlockFace::Top),
            Some(BlockId(5)),
            |_| None,
        );
        assert!(s.place.is_none());
    }

    #[test]
    fn place_true_replaceable_destination_emits_at_face_normal() {
        let s = step_placement(
            true,
            &target_at(BlockPos::new(3, 64, 5), BlockFace::Top),
            Some(BlockId(7)),
            |_| Some(true),
        );
        let (pos, block_id) = s.place.expect("expected a placement");
        assert_eq!(block_id, BlockId(7));
        assert_eq!(pos, BlockPos::new(3, 65, 5));
    }
}
