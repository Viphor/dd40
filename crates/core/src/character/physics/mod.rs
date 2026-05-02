//! Physics vocabulary re-exports.
//!
//! All physics component types, system sets, and the config resource are
//! defined in [`dd40_physics_core`].  This module re-exports them so existing
//! code using `dd40_core::character::physics::Aabb` etc. continues to work.
//!
//! The [`CollisionShape`] type lives in [`crate::block`] (it is part of
//! [`BlockDefinition`]) and is re-exported here for convenience.

pub use dd40_physics_core::prelude::{
    Aabb, CharacterCollider, CharacterPosition, GravityScale, Grounded, Impulse,
    PhysicsBody, PhysicsConfig, PhysicsCorePlugin, PhysicsSet, TentativePosition, Velocity,
};

/// Re-exported for backward compatibility. The canonical definition lives in
/// [`crate::block::CollisionShape`] — use that path in new code.
pub use crate::block::CollisionShape;
