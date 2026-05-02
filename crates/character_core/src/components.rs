use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Marker component for the locally-controlled player entity.
///
/// Exactly one entity in a running client session should carry this marker.
/// Systems that need to distinguish the player from other characters (camera,
/// crosshair, block-interaction highlights) filter with [`With<Player>`].
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Player;

/// Marker component shared by all physics-simulated humanoid entities —
/// players, NPCs, and remote players alike.
///
/// Systems that process ALL characters (replication, collision, animation)
/// filter with [`With<Character>`].  Player-exclusive systems add
/// [`With<Player>`] on top.
#[derive(Debug, Default, Component, Reflect, PartialEq, Eq, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Character;

/// Walking speed of the character in world units per second.
///
/// The character controller multiplies this by
/// [`CharacterController::sprint_multiplier`] when
/// [`CharacterInput::sprint`] is `true`.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

/// Upward velocity (world units per second) applied on a successful jump.
///
/// Entities **without** this component cannot jump — [`CharacterInput::jump`]
/// is silently ignored when `JumpImpulse` is absent.  This makes jump
/// capability opt-in so non-player physics bodies don't accidentally gain it.
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct JumpImpulse(pub f32);

impl Default for JumpImpulse {
    fn default() -> Self {
        Self(8.0)
    }
}

/// Singleton resource that stores the world position at which the local player
/// should be spawned (or re-spawned after death).
#[derive(Resource)]
pub struct SpawnPosition(pub Vec3);
