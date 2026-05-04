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
/// One-shot fields are reset to `false` after being processed:
///
/// - [`jump`](Self::jump) — by the character-physics controller system
/// - [`interact`](Self::interact) — by the interaction layer
/// - [`place`](Self::place) — by the placement layer
///
/// All other fields persist between ticks. In particular,
/// [`attack`](Self::attack) is **continuous**: while held it drives the
/// mining state machine (and, in the future, melee attacks) every tick.
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
    /// Continuous "primary action" / attack intent. While `true`, the mining
    /// system attempts to mine whatever the character currently targets, and
    /// (once melee combat lands) drives swing animations and damage. Held
    /// across ticks like [`sprint`](Self::sprint); the controller does not
    /// reset it.
    pub attack: bool,
    /// One-shot "secondary action" / interact intent — flipping a lever,
    /// pressing a button, opening a container. The character-interaction
    /// layer resets it to `false` after one attempt (success or failure),
    /// matching the [`jump`](Self::jump) convention.
    pub interact: bool,
    /// One-shot "place from active item" intent. Distinct from
    /// [`interact`](Self::interact) so the local-player input layer can
    /// decide ahead of time whether right-click should interact with the
    /// targeted block (lever/button/chest) or place the held item — keeping
    /// that game-UX policy out of the character/interaction layer. Reset
    /// after one attempt.
    pub place: bool,
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
