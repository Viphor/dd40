use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::{
    block_collision::BlockCollisionPlugin,
    character_collision::CharacterCollisionPlugin,
    integration::IntegrationPlugin,
};

/// Registers all physics simulation systems.
///
/// This is an **implementation** plugin: it contains the gravity, collision,
/// and integration systems.  It depends on [`PhysicsCorePlugin`] (vocabulary)
/// and [`CorePlugin`] (block registry, chunk types), both of which are
/// auto-added via [`dd40_core::ensure_plugins!`] if not already present.
///
/// Adding only `PhysicsPlugin` to your [`App`] is sufficient — you do not need
/// to add `CorePlugin` or `PhysicsCorePlugin` manually.
#[derive(Default)]
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);
        app.add_plugins((IntegrationPlugin, BlockCollisionPlugin, CharacterCollisionPlugin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockRegistry;
    use dd40_physics_core::prelude::PhysicsConfig;

    #[test]
    fn physics_plugin_auto_adds_dependencies() {
        let mut app = App::new();
        app.add_plugins(PhysicsPlugin);
        assert!(
            app.world().contains_resource::<BlockRegistry>(),
            "CorePlugin must be auto-added by PhysicsPlugin"
        );
        assert!(
            app.world().contains_resource::<PhysicsConfig>(),
            "PhysicsCorePlugin must be auto-added by PhysicsPlugin"
        );
    }

    #[test]
    fn physics_plugin_is_idempotent_with_core_already_added() {
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        app.add_plugins(PhysicsPlugin);
        assert!(app.world().contains_resource::<PhysicsConfig>());
    }
}
