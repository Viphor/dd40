use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub mod builder;
pub mod controller;
pub mod physics;
pub mod plugin;


/// Ordering anchor for render-frame visual systems.
///
/// Both `dd40_network` and `dd40_player` import this set so they can enforce
/// a deterministic order between frame interpolation and camera-follow without
/// a direct crate dependency on each other.
///
/// **Expected order (both in `Update`):**
/// 1. `CharacterRenderSet::FrameInterpolation` — write the smoothed `Transform`
/// 2. `CharacterRenderSet::CameraSync` — follow the now-smoothed `Transform`
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharacterRenderSet {
    /// Write the visual `Transform` for predicted characters.
    ///
    /// Frame-interpolation and visual-correction application belong here.
    FrameInterpolation,
    /// Sync the camera (or any other follower) to the player `Transform`.
    ///
    /// Always runs **after** [`CharacterRenderSet::FrameInterpolation`].
    CameraSync,
}

/// Marker component that identifies the player entity.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Player;

#[derive(Debug, Default, Component, Reflect, PartialEq, Eq, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Character;

/// Walking / flying speed of the character in units per second.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

/// Upward velocity (in world units per second) applied when the character
/// jumps.
///
/// Entities **without** this component cannot jump — [`CharacterInput::jump`]
/// is silently ignored when `JumpImpulse` is absent.  This makes jump
/// capability opt-in: non-player physics bodies (crates, projectiles) don't
/// accidentally gain jump ability.
///
/// [`CharacterInput::jump`]: controller::CharacterInput::jump
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct JumpImpulse(pub f32);

impl Default for JumpImpulse {
    fn default() -> Self {
        Self(8.0)
    }
}

#[derive(Resource)]
pub struct SpawnPosition(pub Vec3);

#[derive(Bundle)]
pub struct CharacterBundle {
    pub character: Character,
    pub movement_speed: MovementSpeed,
    pub transform: Transform,
    pub name: Name,
}

impl CharacterBundle {
    pub fn new(
        name: impl Into<String>,
        movement_speed: MovementSpeed,
        transform: Transform,
    ) -> Self {
        Self {
            character: Character,
            movement_speed,
            transform,
            name: Name::new(name.into()),
        }
    }
}

impl Default for CharacterBundle {
    fn default() -> Self {
        Self {
            character: Character,
            movement_speed: MovementSpeed::default(),
            transform: Transform::default(),
            name: Name::new("Character"),
        }
    }
}
