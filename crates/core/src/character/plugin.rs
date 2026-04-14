use bevy::prelude::*;

use crate::character::{Character, MovementSpeed, Player, physics::PhysicsPlugin};

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugin)
            .register_type::<Character>()
            .register_type::<MovementSpeed>()
            .register_type::<Player>();
    }
}
