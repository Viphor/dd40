//! Glue crate that integrates [`dd40_character_core`] with
//! [`dd40_physics_core`].
//!
//! # Overview
//!
//! Following the `dd40_integration_<source>_<destination>` naming
//! scheme, this crate translates *character intent* — expressed as
//! [`dd40_character_core::CharacterInput`] and tuned by
//! [`dd40_character_core::CharacterController`] — into *physics
//! output* — written to [`dd40_physics_core::Impulse`] and read by
//! the physics integrator.
//!
//! Neither [`dd40_character_core`] nor [`dd40_physics_core`] depend
//! on each other; this crate is the only place in the codebase that
//! knows about both sides at once. Modders swapping in an alternative
//! physics engine reimplement this crate (and the matching
//! `dd40_integration_<other>_*` crates) without touching the
//! foundation vocabulary on either side.
//!
//! # Usage
//!
//! Add [`plugin::IntegrationCharacterPhysicsPlugin`] to your [`App`]
//! to enable character-driven movement:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_integration_character_physics::plugin::IntegrationCharacterPhysicsPlugin;
//!
//! App::new()
//!     .add_plugins(IntegrationCharacterPhysicsPlugin)
//!     .run();
//! ```

pub mod plugin;

pub use plugin::IntegrationCharacterPhysicsPlugin;
