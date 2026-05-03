use bevy::prelude::*;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::{
    components::{Character, JumpImpulse, MovementSpeed, Player, PlayerId},
    controller::{CharacterController, CharacterControllerPlugin, CharacterInput},
    mining_state::MiningState,
    system_sets::CharacterRenderSet,
};

/// Foundation plugin that registers all character vocabulary and wires the
/// character controller into the physics schedule.
///
/// Depends on [`dd40_physics_core::PhysicsCorePlugin`], which is auto-added
/// via `ensure_plugins!` inside [`CharacterControllerPlugin`].
///
/// Use [`dd40_physics::PhysicsPlugin`] (or add `PhysicsPlugin` +
/// `CharacterCorePlugin` together) for a fully-simulated character.
#[derive(Default)]
pub struct CharacterCorePlugin;

impl Plugin for CharacterCorePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, PhysicsCorePlugin);

        app.register_type::<Character>()
            .register_type::<Player>()
            .register_type::<PlayerId>()
            .register_type::<MovementSpeed>()
            .register_type::<JumpImpulse>()
            .register_type::<MiningState>()
            .register_type::<CharacterInput>()
            .register_type::<CharacterController>()
            .configure_sets(
                Update,
                (
                    CharacterRenderSet::FrameInterpolation,
                    CharacterRenderSet::CameraSync,
                )
                    .chain(),
            )
            .add_plugins(CharacterControllerPlugin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_physics_core::prelude::PhysicsConfig;

    #[test]
    fn character_core_plugin_auto_adds_physics_core() {
        let mut app = App::new();
        app.add_plugins(CharacterCorePlugin);
        assert!(
            app.world().contains_resource::<PhysicsConfig>(),
            "PhysicsCorePlugin must be auto-added by CharacterCorePlugin"
        );
    }

    #[test]
    fn character_core_plugin_is_idempotent_with_physics_core_already_added() {
        let mut app = App::new();
        app.add_plugins(dd40_physics_core::plugin::PhysicsCorePlugin);
        app.add_plugins(CharacterCorePlugin);
        assert!(app.world().contains_resource::<PhysicsConfig>());
    }
}
