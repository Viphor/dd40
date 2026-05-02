use bevy::prelude::*;

/// Mouse sensitivity for first-person look.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self(0.002)
    }
}

/// Pitch and yaw angles for the camera entity.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct CameraRotation {
    pub pitch: f32,
    pub yaw: f32,
}

impl Default for CameraRotation {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}
