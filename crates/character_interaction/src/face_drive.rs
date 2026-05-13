//! Drives non-local-player [`CharacterFace`] orientation from the
//! parent character's [`CharacterInput`].
//!
//! On the local-player client the face is driven by `mouse_look` in
//! `dd40_player_input` directly from raw mouse motion. That entity
//! is identified by the [`Player`] marker and is excluded here so the
//! lower-frequency `CharacterInput` path does not fight `mouse_look`
//! and produce visible snap-back between mouse samples.
//!
//! For every other character — server-side characters and remote
//! characters on a client — this is the **only** writer of the face
//! transform: without it, faces stay at identity rotation and the
//! targeting raycast never hits the block the player is actually
//! looking at, silently breaking server-side mining and placement.

use bevy::prelude::*;
use dd40_character_core::components::{Character, Player};
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::face::{CameraRotation, CharacterFace};

/// Reads each character's [`CharacterInput::pitch`] / [`CharacterInput::yaw`]
/// and writes the resulting orientation onto the character's
/// [`CharacterFace`] child — both [`CameraRotation`] and
/// `Transform.rotation` so downstream consumers (the targeting raycast,
/// camera mirror, future face-mesh renderer) see a consistent value.
///
/// Faces whose parent character carries the [`Player`] marker (the
/// local player on a client) are skipped: `mouse_look` already drives
/// those faces directly from raw mouse motion at sub-tick latency, and
/// re-deriving from the lower-frequency `CharacterInput` would cause
/// visible snap-back between mouse samples.
///
/// Skips faces whose parent is missing or is not a [`Character`].
pub(crate) fn drive_face_from_input(
    character_query: Query<&CharacterInput, (With<Character>, Without<Player>)>,
    mut face_query: Query<(&mut Transform, &mut CameraRotation, &ChildOf), With<CharacterFace>>,
) {
    for (mut transform, mut rotation, child_of) in &mut face_query {
        let Ok(input) = character_query.get(child_of.parent()) else {
            continue;
        };
        rotation.pitch = input.pitch;
        rotation.yaw = input.yaw;
        transform.rotation = Quat::from_euler(EulerRot::YXZ, input.yaw, input.pitch, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_character_core::components::{Character, Player};
    use dd40_character_core::face::CharacterFace;

    fn build_app() -> App {
        let mut app = App::new();
        app.add_systems(Update, drive_face_from_input);
        app
    }

    #[test]
    fn face_transform_follows_character_input_yaw() {
        let mut app = build_app();
        let face = app
            .world_mut()
            .spawn((
                CharacterFace::default(),
                CameraRotation::default(),
                Transform::default(),
            ))
            .id();
        let body = app
            .world_mut()
            .spawn((
                Character,
                CharacterInput {
                    yaw: std::f32::consts::PI,
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().entity_mut(face).insert(ChildOf(body));

        app.update();

        let rot = app.world().get::<Transform>(face).unwrap().rotation;
        let expected = Quat::from_euler(EulerRot::YXZ, std::f32::consts::PI, 0.0, 0.0);
        assert!(rot.abs_diff_eq(expected, 1e-5));
        let cam = app.world().get::<CameraRotation>(face).unwrap();
        assert!((cam.yaw - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn local_player_face_is_left_alone() {
        let mut app = build_app();
        let face = app
            .world_mut()
            .spawn((
                CharacterFace::default(),
                CameraRotation::default(),
                Transform::default(),
            ))
            .id();
        let body = app
            .world_mut()
            .spawn((
                Character,
                Player,
                CharacterInput {
                    yaw: std::f32::consts::PI,
                    ..Default::default()
                },
            ))
            .id();
        app.world_mut().entity_mut(face).insert(ChildOf(body));

        app.update();

        let rot = app.world().get::<Transform>(face).unwrap().rotation;
        assert_eq!(rot, Quat::IDENTITY);
    }

    #[test]
    fn face_without_character_parent_is_unchanged() {
        let mut app = build_app();
        let other = app.world_mut().spawn(()).id();
        let face = app
            .world_mut()
            .spawn((
                CharacterFace::default(),
                CameraRotation::default(),
                Transform::default(),
                ChildOf(other),
            ))
            .id();
        app.update();
        let rot = app.world().get::<Transform>(face).unwrap().rotation;
        assert_eq!(rot, Quat::IDENTITY);
    }
}
