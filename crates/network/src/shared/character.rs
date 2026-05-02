use dd40_character_core::controller::{CharacterController, CharacterInput};
use dd40_character_core::components::JumpImpulse;
use dd40_physics_core::prelude::{Aabb, CharacterCollider, PhysicsBody};
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
}

pub(crate) fn character_bundle() -> (
    CharacterInput,
    PhysicsBody,
    CharacterCollider,
    Aabb,
    JumpImpulse,
    CharacterController,
) {
    (
        CharacterInput::default(),
        PhysicsBody,
        CharacterCollider,
        Aabb::player(),
        JumpImpulse::default(),
        CharacterController::default(),
    )
}
