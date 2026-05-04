//! Character controller — per-tick intent + tuning components.
//!
//! Any system (player input, AI, network replication) that wants to move a
//! character writes its intent into [`CharacterInput`]. The translation from
//! intent to physics forces lives in
//! `dd40_integration_character_physics::controller::apply_character_controller`,
//! which keeps this foundation crate free of any direct dependency on
//! [`dd40_physics_core`].

use bevy::prelude::*;

// ---------------------------------------------------------------------------
// CharacterInput
// ---------------------------------------------------------------------------

/// Per-tick movement intent written by any input source (local player, AI,
/// network replication) and consumed by the character-physics integration
/// crate every [`FixedUpdate`] tick.
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
    /// One-shot jump request. Requires [`crate::components::JumpImpulse`] on
    /// the entity. Reset to `false` after processing.
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
/// Paired with [`CharacterInput`] (auto-inserted via `#[require]`) and read by
/// `dd40_integration_character_physics::controller::apply_character_controller`.
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
