use bevy::prelude::*;

use crate::character::{Character, JumpImpulse, MovementSpeed, Player, controller::CharacterControllerPlugin};

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CharacterControllerPlugin)
            .register_type::<Character>()
            .register_type::<MovementSpeed>()
            .register_type::<JumpImpulse>()
            .register_type::<Player>();
    }
}
