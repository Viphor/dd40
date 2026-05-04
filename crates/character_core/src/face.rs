//! Per-character "face" anchor — the eye position and look direction.
//!
//! Every [`Character`][crate::components::Character] entity owns a child
//! entity that carries [`CharacterFace`] and [`CameraRotation`]. The face is
//! the point that:
//!
//! - the targeting raycast in `dd40_character_interaction` originates from,
//! - the local-player camera mirrors,
//! - the mouse-look input writes pitch/yaw into,
//! - a future face-mesh renderer can anchor to.
//!
//! Putting the face on its own entity (rather than deriving an eye position
//! from the character's collider, or piggy-backing on the local `Camera3d`)
//! lets the same code work on the headless server (no camera) and lets
//! different characters declare different eye heights.
//!
//! ## Wiring
//!
//! [`CharacterBuilder::spawn`][crate::builder::CharacterBuilder::spawn] is
//! the canonical way to spawn a character — it attaches the face child for
//! you. If you spawn manually, the face must be a child entity (Bevy 0.18
//! [`ChildOf`]) of the character body, with [`CharacterFace`],
//! [`CameraRotation`], and a local [`Transform`] whose translation equals
//! the desired eye offset.
//!
//! [`ChildOf`]: bevy::prelude::ChildOf

use bevy::prelude::*;

/// Default eye-height offset for a humanoid character, in metres above the
/// body's origin. Matches the previous hard-coded value used by the local
/// camera-sync system.
pub const DEFAULT_FACE_OFFSET: Vec3 = Vec3::new(0.0, 1.6, 0.0);

/// Marks a child entity as a character's "face" — the eye / head anchor.
///
/// Attached to a child of every [`Character`][crate::components::Character]
/// entity. The child's local [`Transform::translation`] holds the eye
/// offset (typically [`DEFAULT_FACE_OFFSET`]); the child's local rotation is
/// driven by [`CameraRotation`] (pitch + yaw).
///
/// The `offset` field is informational only — the source of truth at
/// runtime is the entity's `Transform.translation`. It is exposed so
/// systems and editors can inspect the configured eye height without
/// re-parenting maths.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct CharacterFace {
    /// Local-space offset from the character body to the face/eye position.
    pub offset: Vec3,
}

impl Default for CharacterFace {
    fn default() -> Self {
        Self {
            offset: DEFAULT_FACE_OFFSET,
        }
    }
}

/// Pitch and yaw angles for a character's face.
///
/// Lives on the face entity (not the camera) so the same data flows to any
/// observer: the local camera mirrors it, the targeting raycast reads its
/// resulting `GlobalTransform`, and the body's yaw can be derived from
/// `yaw` if a controller wants the body to rotate with the head.
///
/// Both angles are in radians. `pitch` is conventionally clamped to
/// `(-π/2, π/2)` by whichever system writes it (see `mouse_look` in
/// `dd40_player_movement`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct CameraRotation {
    /// Look pitch in radians (positive = up).
    pub pitch: f32,
    /// Look yaw in radians (positive = left, around +Y).
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

/// Mouse sensitivity for first-person look, in radians per pixel of mouse
/// motion. Lives on the face entity so it can be configured per-character
/// (e.g. an NPC face entity simply omits it).
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self(0.002)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_face_default_uses_humanoid_eye_height() {
        let face = CharacterFace::default();
        assert_eq!(face.offset, Vec3::new(0.0, 1.6, 0.0));
        assert_eq!(face.offset, DEFAULT_FACE_OFFSET);
    }

    #[test]
    fn camera_rotation_default_is_zero() {
        let r = CameraRotation::default();
        assert_eq!(r.pitch, 0.0);
        assert_eq!(r.yaw, 0.0);
    }

    #[test]
    fn mouse_sensitivity_default_is_two_thousandths() {
        assert_eq!(MouseSensitivity::default().0, 0.002);
    }
}
