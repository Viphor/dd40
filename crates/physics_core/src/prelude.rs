pub use crate::{
    character_ext::{CharacterPhysicsConfig, CharacterPhysicsExt},
    components::{
        Aabb, CharacterCollider, CharacterPosition, GravityScale, Grounded, Impulse, PhysicsBody,
        Velocity,
    },
    plugin::PhysicsCorePlugin,
    resources::{CharacterSpatialCache, PhysicsConfig},
    system_sets::PhysicsSet,
};
