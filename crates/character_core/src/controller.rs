//! Character controller — the bridge between input systems and the physics pipeline.
//!
//! Any system (player input, AI, network replication) that wants to move a
//! character writes its intent into [`CharacterInput`].  A single physics-side
//! system ([`apply_character_controller`]) then translates that intent into
//! [`dd40_physics_core::Impulse`] changes before [`PhysicsSet::Integrate`]
//! runs, so gravity, block collision, and character-vs-character collision all
//! act on the resulting velocity as normal.

use bevy::prelude::*;
use dd40_physics_core::prelude::{
    Grounded, Impulse, PhysicsBody, PhysicsCorePlugin, PhysicsSet, Velocity,
};

use crate::components::{JumpImpulse, MovementSpeed};

// ---------------------------------------------------------------------------
// CharacterInput
// ---------------------------------------------------------------------------

/// Per-tick movement intent written by any input source (local player, AI,
/// network replication) and consumed by [`apply_character_controller`] every
/// [`FixedUpdate`] tick.
///
/// ## Reset semantics
///
/// Only [`jump`] is a one-shot flag — it is reset to `false` after being
/// processed.  All other fields persist between ticks.
///
/// [`jump`]: CharacterInput::jump
#[derive(Debug, Default, Clone, PartialEq, Component, Reflect)]
#[reflect(Component)]
pub struct CharacterInput {
    /// Desired movement direction in world space (XZ plane), normalised.
    pub movement: Vec3,
    /// One-shot jump request. Requires [`JumpImpulse`] on the entity.
    /// Reset to `false` after processing.
    pub jump: bool,
    /// Whether the character is sprinting this tick.
    pub sprint: bool,
    /// Camera pitch in radians. Does not affect physics; carried for replication.
    pub pitch: f32,
    /// Camera yaw in radians.
    pub yaw: f32,
}

// ---------------------------------------------------------------------------
// CharacterController
// ---------------------------------------------------------------------------

/// Physics tuning parameters for a character.
///
/// Paired with [`CharacterInput`] (auto-inserted via `#[require]`) to drive
/// [`apply_character_controller`].
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
#[require(CharacterInput)]
pub struct CharacterController {
    /// Speed multiplier when [`CharacterInput::sprint`] is `true`.
    pub sprint_multiplier: f32,
    /// Fraction of movement correction applied when airborne (0 = no control,
    /// 1 = full ground control).
    pub air_control: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            sprint_multiplier: 2.0,
            air_control: 0.3,
        }
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

fn apply_character_controller(
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
            if grounded.is_grounded() {
                if let Some(ji) = jump_impulse {
                    impulse.0.y += ji.0;
                }
            }
            input.jump = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers [`CharacterInput`] and [`CharacterController`] and wires
/// [`apply_character_controller`] into the schedule.
///
/// Auto-added by [`super::plugin::CharacterCorePlugin`].
pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, PhysicsCorePlugin);

        app.register_type::<CharacterInput>()
            .register_type::<CharacterController>()
            .add_systems(
                FixedUpdate,
                apply_character_controller
                    .after(PhysicsSet::InputSync)
                    .before(PhysicsSet::Integrate),
            );
    }
}
