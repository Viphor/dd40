use bevy::prelude::*;

use crate::{bundles::CharacterBundle, components::MovementSpeed};

/// Fluent builder for [`CharacterBundle`].
///
/// Lets callers chain optional overrides rather than constructing the bundle
/// struct directly, keeping spawn sites readable as the bundle grows.
///
/// # Example
///
/// ```
/// use dd40_character_core::builder::CharacterBuilder;
/// use bevy::math::Vec3;
///
/// let bundle = CharacterBuilder::new("Player")
///     .movement_speed(6.0)
///     .transform(bevy::prelude::Transform::from_translation(Vec3::new(0.0, 64.0, 0.0)))
///     .build();
/// ```
pub struct CharacterBuilder {
    name: String,
    movement_speed: MovementSpeed,
    transform: Transform,
}

impl CharacterBuilder {
    /// Starts a builder with default speed and the world origin as spawn point.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            movement_speed: MovementSpeed::default(),
            transform: Transform::default(),
        }
    }

    /// Overrides the base movement speed (world units per second).
    pub fn movement_speed(mut self, speed: f32) -> Self {
        self.movement_speed = MovementSpeed(speed);
        self
    }

    /// Overrides the initial world-space transform.
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Consumes the builder and produces the [`CharacterBundle`].
    pub fn build(self) -> impl Bundle {
        CharacterBundle::new(self.name, self.movement_speed, self.transform)
    }
}
