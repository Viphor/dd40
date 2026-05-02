use bevy::prelude::*;

use crate::components::{Character, MovementSpeed};

/// Convenience bundle that groups the components every character entity needs
/// at spawn time.
///
/// Does **not** include [`dd40_physics_core::PhysicsBody`] — add that
/// separately when the entity should participate in the physics simulation.
#[derive(Bundle)]
pub struct CharacterBundle {
    /// Marks this entity as a character (used as a query filter).
    pub character: Character,
    /// Base movement speed in world units per second.
    pub movement_speed: MovementSpeed,
    /// World-space position and orientation.
    pub transform: Transform,
    /// Human-readable debug name.
    pub name: Name,
}

impl CharacterBundle {
    /// Creates a bundle with the given name, speed, and transform.
    pub fn new(
        name: impl Into<String>,
        movement_speed: MovementSpeed,
        transform: Transform,
    ) -> Self {
        Self {
            character: Character,
            movement_speed,
            transform,
            name: Name::new(name.into()),
        }
    }
}

impl Default for CharacterBundle {
    fn default() -> Self {
        Self {
            character: Character,
            movement_speed: MovementSpeed::default(),
            transform: Transform::default(),
            name: Name::new("Character"),
        }
    }
}
