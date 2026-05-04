//! Per-character "interact" / secondary-action handling.
//!
//! Driven by [`CharacterInput::interact`]. This module owns the seam where
//! future block-interaction behaviours (levers, buttons, opening containers)
//! will plug in. For now the system is a stub that simply consumes the
//! `interact` flag every tick where it was `true`, mirroring how `place` is
//! consumed by [`crate::placement::try_place_block`].
//!
//! The point of landing the system now — even as a stub — is that the
//! protocol/replication shape and the input layer's translation policy are
//! both correct from day one: modders adding a "lever flips" feature only
//! need to extend this module rather than re-plumb input.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::targeted_block::TargetedBlock;

/// Per-character interact system.
///
/// Reads `(&mut CharacterInput, &TargetedBlock)` for every [`Character`]
/// and resets `CharacterInput::interact` to `false` after observing it.
/// No block-interaction behaviours are wired up yet — see module docs.
pub(crate) fn try_interact(
    mut character_query: Query<(&mut CharacterInput, &TargetedBlock), With<Character>>,
) {
    for (mut input, targeted) in &mut character_query {
        if !input.interact {
            continue;
        }
        debug!(
            "Interact intent fired (target = {:?}); no behaviour wired yet",
            targeted.pos
        );
        input.interact = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_character_core::components::Character;
    use dd40_character_core::targeted_block::TargetedBlock;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_systems(Update, try_interact);
        app
    }

    #[test]
    fn interact_true_is_reset_after_one_tick() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                Character,
                CharacterInput {
                    interact: true,
                    ..Default::default()
                },
                TargetedBlock::default(),
            ))
            .id();

        app.update();

        let input = app.world().get::<CharacterInput>(entity).unwrap();
        assert!(!input.interact, "interact should have been consumed");
    }

    #[test]
    fn interact_false_is_left_alone() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                Character,
                CharacterInput::default(),
                TargetedBlock::default(),
            ))
            .id();

        app.update();

        let input = app.world().get::<CharacterInput>(entity).unwrap();
        assert!(!input.interact);
    }

    #[test]
    fn interact_does_not_affect_other_input_fields() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn((
                Character,
                CharacterInput {
                    interact: true,
                    attack: true,
                    place: true,
                    jump: true,
                    sprint: true,
                    yaw: 1.0,
                    pitch: 2.0,
                    movement: Vec3::new(1.0, 0.0, 0.0),
                },
                TargetedBlock::default(),
            ))
            .id();

        app.update();

        let input = app.world().get::<CharacterInput>(entity).unwrap();
        assert!(!input.interact);
        assert!(input.attack, "attack must not be touched by try_interact");
        assert!(input.place, "place must not be touched by try_interact");
        assert!(input.jump);
        assert!(input.sprint);
        assert_eq!(input.yaw, 1.0);
        assert_eq!(input.pitch, 2.0);
        assert_eq!(input.movement, Vec3::new(1.0, 0.0, 0.0));
    }
}
