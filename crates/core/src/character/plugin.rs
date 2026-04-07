use bevy::prelude::*;

use crate::character::{Character, MovementSpeed, Player};

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Character>()
            .register_type::<MovementSpeed>()
            .register_type::<Player>();
    }
}
