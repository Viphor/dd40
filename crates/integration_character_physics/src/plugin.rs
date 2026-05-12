//! Root plugin for the `dd40_integration_character_physics` crate.
//!
//! [`IntegrationCharacterPhysicsPlugin`] is the single entry point
//! for this integration crate. Add it to your [`App`] once to wire
//! character intent into the physics pipeline.
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_integration_character_physics::plugin::IntegrationCharacterPhysicsPlugin;
//!
//! App::new()
//!     .add_plugins(IntegrationCharacterPhysicsPlugin)
//!     .run();
//! ```

use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::controller;

/// Plugin that bridges [`dd40_character_core`] intent into
/// [`dd40_physics_core`] forces.
///
/// ## What this plugin sets up
///
/// - Ensures [`CorePlugin`], [`CharacterCorePlugin`], and
///   [`PhysicsCorePlugin`] are added.
/// - Registers [`controller::apply_character_controller`] in
///   [`FixedUpdate`] between [`dd40_physics_core::PhysicsSet::InputSync`]
///   and [`dd40_physics_core::PhysicsSet::Integrate`].
#[derive(Default)]
pub struct IntegrationCharacterPhysicsPlugin;

impl Plugin for IntegrationCharacterPhysicsPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            CharacterCorePlugin,
            PhysicsCorePlugin
        );
        controller::add_systems(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use dd40_character_core::components::{JumpImpulse, MovementSpeed};
    use dd40_character_core::controller::{CharacterController, CharacterInput};
    use dd40_physics_core::prelude::{Grounded, Impulse, PhysicsBody, Velocity};

    fn spawn_character_app() -> App {
        let mut app = App::new();
        app.add_plugins(IntegrationCharacterPhysicsPlugin);
        app
    }

    #[test]
    fn plugin_builds_in_a_minimal_app() {
        let mut app = spawn_character_app();
        app.update();
    }

    #[test]
    fn grounded_movement_applies_full_horizontal_impulse() {
        let mut app = spawn_character_app();
        let entity = app
            .world_mut()
            .spawn((
                MovementSpeed(5.0),
                CharacterController::default(),
                CharacterInput {
                    movement: Vec3::new(1.0, 0.0, 0.0),
                    ..default()
                },
                PhysicsBody,
                Grounded(true),
                Velocity(Vec3::ZERO),
                Impulse(Vec3::ZERO),
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);

        let impulse = app.world().get::<Impulse>(entity).unwrap();
        assert!(
            (impulse.0.x - 5.0).abs() < 1e-5,
            "expected impulse.x ≈ 5.0, got {}",
            impulse.0.x
        );
        assert!(impulse.0.z.abs() < 1e-5);
        assert!(impulse.0.y.abs() < 1e-5);
    }

    #[test]
    fn airborne_movement_is_scaled_by_air_control() {
        let mut app = spawn_character_app();
        let entity = app
            .world_mut()
            .spawn((
                MovementSpeed(10.0),
                CharacterController {
                    air_control: 0.25,
                    ..default()
                },
                CharacterInput {
                    movement: Vec3::new(0.0, 0.0, 1.0),
                    ..default()
                },
                PhysicsBody,
                Grounded(false),
                Velocity(Vec3::ZERO),
                Impulse(Vec3::ZERO),
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);

        let impulse = app.world().get::<Impulse>(entity).unwrap();
        assert!((impulse.0.z - 2.5).abs() < 1e-5, "got {}", impulse.0.z);
    }

    #[test]
    fn jump_consumes_one_shot_only_when_grounded() {
        let mut app = spawn_character_app();
        let entity = app
            .world_mut()
            .spawn((
                MovementSpeed(0.0),
                CharacterController::default(),
                CharacterInput {
                    jump: true,
                    ..default()
                },
                JumpImpulse(8.0),
                PhysicsBody,
                Grounded(true),
                Velocity(Vec3::ZERO),
                Impulse(Vec3::ZERO),
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);

        let impulse = app.world().get::<Impulse>(entity).unwrap();
        assert!(
            (impulse.0.y - 8.0).abs() < 1e-5,
            "expected jump impulse 8.0, got {}",
            impulse.0.y
        );
        let input = app.world().get::<CharacterInput>(entity).unwrap();
        assert!(!input.jump, "jump should be reset after processing");
    }
}
