use bevy::prelude::*;

use crate::{
    components::{Character, JumpImpulse, MovementSpeed, Player, PlayerId},
    controller::{CharacterController, CharacterInput},
    face::{CameraRotation, CharacterFace, MouseSensitivity},
    mining_state::MiningState,
    system_sets::CharacterRenderSet,
    targeted_block::{BlockFace, TargetedBlock},
};

/// Foundation plugin that registers all character vocabulary.
///
/// This plugin is a *types-only* foundation plugin — it has no behaviour
/// systems. The translation of [`CharacterInput`] into physics forces lives
/// in `dd40_integration_character_physics::IntegrationCharacterPhysicsPlugin`,
/// which any binary that wants character locomotion must add.
#[derive(Default)]
pub struct CharacterCorePlugin;

impl Plugin for CharacterCorePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, dd40_core::plugin::CorePlugin);

        app.register_type::<Character>()
            .register_type::<Player>()
            .register_type::<PlayerId>()
            .register_type::<MovementSpeed>()
            .register_type::<JumpImpulse>()
            .register_type::<MiningState>()
            .register_type::<BlockFace>()
            .register_type::<TargetedBlock>()
            .register_type::<CharacterFace>()
            .register_type::<CameraRotation>()
            .register_type::<MouseSensitivity>()
            .register_type::<CharacterInput>()
            .register_type::<CharacterController>()
            .configure_sets(
                Update,
                (
                    CharacterRenderSet::FrameInterpolation,
                    CharacterRenderSet::CameraSync,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_core_plugin_builds_without_physics_dep() {
        let mut app = App::new();
        app.add_plugins(CharacterCorePlugin);
        app.update();
    }
}
