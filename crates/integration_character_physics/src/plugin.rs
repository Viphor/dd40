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

/// Plugin that bridges [`dd40_character_core`] intent into
/// [`dd40_physics_core`] forces.
///
/// ## What this plugin sets up
///
/// - Ensures [`CorePlugin`], [`CharacterCorePlugin`], and
///   [`PhysicsCorePlugin`] are added.
/// - **TODO** (`extract-character-controller-system`): take ownership
///   of `apply_character_controller`, currently still living in
///   `dd40_character_core::controller`.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;

    #[test]
    fn plugin_builds_in_a_minimal_app() {
        let mut app = App::new();
        app.add_plugins(IntegrationCharacterPhysicsPlugin);
        app.update();
    }
}
