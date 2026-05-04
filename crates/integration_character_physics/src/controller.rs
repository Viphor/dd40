//! Translates [`CharacterInput`] into physics [`Impulse`].
//!
//! This module is the *only* place in the workspace that reads from
//! [`dd40_character_core`] and writes to [`dd40_physics_core`]. It
//! implements the simple ground-control-with-air-control character
//! locomotion model used by all dd40 characters today.

use bevy::prelude::*;
use dd40_character_core::components::{JumpImpulse, MovementSpeed};
use dd40_character_core::controller::{CharacterController, CharacterInput};
use dd40_physics_core::prelude::{Grounded, Impulse, PhysicsBody, PhysicsSet, Velocity};

/// Reads [`CharacterInput`] / [`CharacterController`] and writes
/// horizontal correction + jump impulses to [`Impulse`].
///
/// Runs in [`FixedUpdate`] between [`PhysicsSet::InputSync`] and
/// [`PhysicsSet::Integrate`] so gravity and collision act on the
/// resulting velocity normally.
///
/// One-shot semantics: [`CharacterInput::jump`] is reset to `false`
/// after each tick.
pub fn apply_character_controller(
    mut query: Query<
        (
            &mut CharacterInput,
            &CharacterController,
            &MovementSpeed,
            &Grounded,
            &Velocity,
            &mut Impulse,
            Option<&JumpImpulse>,
        ),
        With<PhysicsBody>,
    >,
) {
    for (mut input, controller, speed, grounded, velocity, mut impulse, jump_impulse) in
        &mut query
    {
        let effective_speed = speed.0
            * if input.sprint {
                controller.sprint_multiplier
            } else {
                1.0
            };

        let target_h = Vec3::new(
            input.movement.x * effective_speed,
            0.0,
            input.movement.z * effective_speed,
        );
        let current_h = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
        let correction = target_h - current_h;
        let factor = if grounded.is_grounded() {
            1.0
        } else {
            controller.air_control
        };

        impulse.0.x += correction.x * factor;
        impulse.0.z += correction.z * factor;

        if input.jump {
            if grounded.is_grounded()
                && let Some(ji) = jump_impulse
            {
                impulse.0.y += ji.0;
            }
            input.jump = false;
        }
    }
}

/// Schedule wiring for [`apply_character_controller`].
pub(crate) fn add_systems(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        apply_character_controller
            .after(PhysicsSet::InputSync)
            .before(PhysicsSet::Integrate),
    );
}
