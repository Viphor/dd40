use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;

use crate::{
    components::PhysicsConfig,
    components::{
        Aabb, CharacterCollider, CharacterPosition, GravityScale, Grounded, Impulse, PhysicsBody,
        Velocity,
    },
    system_sets::PhysicsSet,
};

/// Registers all physics vocabulary — types, system sets, and the
/// [`PhysicsConfig`] resource.
///
/// This is a **foundation** plugin: it contains no game systems.  Use
/// [`dd40_physics::PhysicsPlugin`] (or a custom alternative) to add the actual
/// simulation systems on top of this vocabulary.
///
/// Added automatically by `PhysicsPlugin` via [`dd40_core::ensure_plugins!`].
#[derive(Default)]
pub struct PhysicsCorePlugin;

impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);

        app.register_type::<Aabb>()
            .register_type::<CharacterPosition>()
            .register_type::<Velocity>()
            .register_type::<Impulse>()
            .register_type::<GravityScale>()
            .register_type::<Grounded>()
            .register_type::<PhysicsBody>()
            .register_type::<CharacterCollider>()
            .register_type::<PhysicsConfig>()
            .init_resource::<PhysicsConfig>()
            .configure_sets(
                FixedUpdate,
                PhysicsSet::InputSync.before(PhysicsSet::Integrate),
            )
            .configure_sets(
                FixedUpdate,
                (
                    PhysicsSet::Integrate,
                    PhysicsSet::BlockCollision,
                    PhysicsSet::CharacterCollision,
                    PhysicsSet::Finalise,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockRegistry;

    #[test]
    fn physics_core_plugin_initialises_config() {
        let mut app = App::new();
        app.add_plugins(PhysicsCorePlugin);
        assert!(
            app.world().contains_resource::<PhysicsConfig>(),
            "PhysicsConfig must be present after adding PhysicsCorePlugin"
        );
    }

    #[test]
    fn physics_core_plugin_auto_adds_core() {
        // CorePlugin is NOT added manually — ensure_plugins! must add it.
        let mut app = App::new();
        app.add_plugins(PhysicsCorePlugin);
        // BlockRegistry is inserted by CorePlugin, so its presence proves
        // CorePlugin was auto-added.
        assert!(
            app.world().contains_resource::<BlockRegistry>(),
            "CorePlugin must be auto-added by PhysicsCorePlugin"
        );
    }

    #[test]
    fn physics_core_plugin_is_idempotent() {
        // Adding PhysicsCorePlugin twice (once manually, once via ensure_plugins!)
        // must not panic with a duplicate-plugin error.
        let mut app = App::new();
        app.add_plugins(CorePlugin);
        app.add_plugins(PhysicsCorePlugin);
        assert!(app.world().contains_resource::<PhysicsConfig>());
    }
}
