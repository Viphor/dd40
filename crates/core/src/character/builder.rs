use bevy::{ecs::bundle::Bundle, transform::components::Transform};

use crate::character::{CharacterBundle, MovementSpeed};

pub struct CharacterBuilder {
    name: String,
    movement_speed: MovementSpeed,
    transform: Transform,
}

impl CharacterBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            movement_speed: MovementSpeed::default(),
            transform: Transform::default(),
        }
    }

    pub fn movement_speed(mut self, speed: f32) -> Self {
        self.movement_speed = MovementSpeed(speed);
        self
    }

    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    pub fn build(self) -> impl Bundle {
        CharacterBundle::new(self.name, self.movement_speed, self.transform)
    }
}
