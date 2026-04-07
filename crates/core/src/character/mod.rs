use bevy::prelude::*;

pub mod plugin;

/// Marker component that identifies the player entity.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Player;

#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Character;

/// Walking / flying speed of the character in units per second.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

#[derive(Bundle)]
pub struct CharacterBundle {
    pub character: Character,
    pub movement_speed: MovementSpeed,
    pub transform: Transform,
    pub name: Name,
}

impl CharacterBundle {
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

    pub fn build(self) -> CharacterBundle {
        CharacterBundle::new(self.name, self.movement_speed, self.transform)
    }
}
