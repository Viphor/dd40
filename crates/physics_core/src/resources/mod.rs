use bevy::prelude::*;

mod spatial_cache;

pub use spatial_cache::CharacterSpatialCache;

/// Global physics configuration resource.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct PhysicsConfig {
    /// Gravitational acceleration in world units/s². Positive pulls downward.
    /// Default: `20.0`.
    pub gravity: f32,
    /// Horizontal velocity damping applied per second when grounded (0 = none,
    /// 1 = instant stop).
    pub ground_friction: f32,
    /// Horizontal velocity damping applied per second when airborne.
    pub air_friction: f32,
    /// Maximum downward speed, in world units/s.
    pub terminal_velocity: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 20.0,
            ground_friction: 1.0,
            air_friction: 0.0002,
            terminal_velocity: 60.0,
        }
    }
}
