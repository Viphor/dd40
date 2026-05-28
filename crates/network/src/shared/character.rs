use dd40_character_core::controller::CharacterInput;
use lightyear::prelude::input::native::ActionState;

use crate::protocol::PlayerInput;

/// Translates one tick's worth of [`PlayerInput`] into [`CharacterInput`] intent.
///
/// This function **must be identical** on server and client.  Any divergence
/// will cause constant rollback corrections on the controlling client.
///
/// # Rules
///
/// - No random numbers, timestamps, or external state — only the input.
/// - Keep the body of this function in sync with every caller site.
///
/// [`PlayerInput`]: crate::protocol::PlayerInput
pub(crate) fn apply_input_to_controller(
    action: &ActionState<PlayerInput>,
    char_input: &mut CharacterInput,
) {
    char_input.movement = action.0.movement;
    char_input.jump = action.0.jump;
    char_input.sprint = action.0.sprint;
    char_input.pitch = action.0.pitch;
    char_input.yaw = action.0.yaw;
    char_input.attack = action.0.attack;
    char_input.interact = action.0.interact;
    char_input.place = action.0.place;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;

    fn action(input: PlayerInput) -> ActionState<PlayerInput> {
        ActionState(input)
    }

    #[test]
    fn propagates_movement_and_camera() {
        let mut ci = CharacterInput::default();
        apply_input_to_controller(
            &action(PlayerInput {
                movement: Vec3::new(1.0, 0.0, 2.0),
                pitch: 0.5,
                yaw: -1.5,
                jump: true,
                sprint: true,
                ..Default::default()
            }),
            &mut ci,
        );
        assert_eq!(ci.movement, Vec3::new(1.0, 0.0, 2.0));
        assert_eq!(ci.pitch, 0.5);
        assert_eq!(ci.yaw, -1.5);
        assert!(ci.jump);
        assert!(ci.sprint);
    }

    #[test]
    fn propagates_action_triple() {
        let mut ci = CharacterInput::default();
        apply_input_to_controller(
            &action(PlayerInput {
                attack: true,
                interact: true,
                place: true,
                ..Default::default()
            }),
            &mut ci,
        );
        assert!(ci.attack);
        assert!(ci.interact);
        assert!(ci.place);
    }

    #[test]
    fn clears_action_triple_when_input_is_false() {
        let mut ci = CharacterInput {
            attack: true,
            interact: true,
            place: true,
            ..Default::default()
        };
        apply_input_to_controller(&action(PlayerInput::default()), &mut ci);
        assert!(!ci.attack, "stale attack must be cleared");
        assert!(!ci.interact, "stale interact must be cleared");
        assert!(!ci.place, "stale place must be cleared");
    }
}
