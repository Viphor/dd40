use bevy::prelude::*;

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
/// Added automatically by `dd40_physics::PhysicsPlugin`.
#[derive(Default)]
pub struct PhysicsCorePlugin;

impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
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
    fn physics_core_plugin_registers_physics_body() {
        let mut app = App::new();
        app.add_plugins(PhysicsCorePlugin);
        assert!(app.world().contains_resource::<PhysicsConfig>());
    }

    #[test]
    fn physics_core_plugin_sets_default_config() {
        let mut app = App::new();
        app.add_plugins(PhysicsCorePlugin);
        let cfg = app.world().resource::<PhysicsConfig>();
        assert!(cfg.gravity > 0.0);
        assert!(cfg.terminal_velocity > 0.0);
    }
}
