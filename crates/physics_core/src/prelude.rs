pub use crate::{
    character_ext::{CharacterPhysicsConfig, CharacterPhysicsExt},
    components::{
        Aabb, GravityScale, Grounded, Impulse, PhysicsBody, PhysicsCollider, PhysicsPosition,
        Velocity,
    },
    plugin::PhysicsCorePlugin,
    resources::{PhysicsConfig, PhysicsSpatialCache},
    system_sets::PhysicsSet,
};
