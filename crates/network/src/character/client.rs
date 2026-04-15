//! Client-side character replication systems.
//!
//! [`ClientCharacterPlugin`] is added automatically by [`NetworkCharacterPlugin`]
//! when the `client` feature is active.

use bevy::prelude::*;
use dd40_core::character::{
    controller::CharacterController,
    physics::PhysicsSet,
};
use lightyear::prelude::{
    Interpolated, Predicted,
    client::input::InputSystems,
    input::native::{ActionState, InputMarker},
};

use crate::protocol::{NetworkCharacter, PlayerInput, PlayerPosition};

use super::apply_input_to_controller;

// ============================================================================
// OBSERVERS
// ============================================================================

/// Attaches [`InputMarker<PlayerInput>`] to a newly-created [`Predicted`]
/// entity that belongs to a network character.
///
/// lightyear creates the `Predicted` entity and copies all registered
/// components (including [`NetworkCharacter`]) onto it before adding the
/// `Predicted` marker.  By the time this observer fires, the entity already
/// has `NetworkCharacter`, so the query is safe.
///
/// `InputMarker` tells lightyear's input pipeline that this entity is the one
/// whose [`ActionState<PlayerInput>`] should be populated from the local
/// player's buffered inputs.
fn on_predicted_character_added(
    trigger: On<Add, Predicted>,
    mut commands: Commands,
    query: Query<(), With<NetworkCharacter>>,
) {
    if query.get(trigger.entity).is_ok() {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<PlayerInput>::default());
        info!(
            "Attached InputMarker<PlayerInput> to predicted character {:?}",
            trigger.entity
        );
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Reads keyboard state and writes it into the [`ActionState<PlayerInput>`]
/// component on the locally-controlled entity.
///
/// Runs in [`FixedPreUpdate`] inside [`InputSystems::WriteClientInputs`] so
/// lightyear picks the input up for the correct tick before sending it to the
/// server.
///
/// Movement is expressed in **local camera space** (−Z = forward, +X = right).
/// If your app needs world-space or camera-relative movement, add your own
/// system in `FixedPreUpdate` at `InputSystems::WriteClientInputs` and remove
/// this one from the app.  Pitch and yaw default to `0.0`; write them from a
/// camera or look-input system if you need rotation replication.
fn buffer_client_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_query: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
) {
    let Ok(mut action) = input_query.single_mut() else {
        return;
    };

    // Local-space directional movement: -Z forward, +X right.
    let mut movement = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        movement.z -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        movement.z += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        movement.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        movement.x -= 1.0;
    }

    action.0 = PlayerInput {
        movement: movement.normalize_or_zero(),
        // Pitch/yaw are preserved from the previous tick so an external system
        // can write them independently without racing this system.
        pitch: action.0.pitch,
        yaw: action.0.yaw,
        jump: keyboard.just_pressed(KeyCode::Space),
        sprint: keyboard.pressed(KeyCode::ControlLeft),
        place_block: false,
        remove_block: false,
    };
}

/// Syncs the replicated [`PlayerPosition`] into [`Transform::translation`]
/// **before** physics runs on the [`Predicted`] entity.
///
/// lightyear's rollback mechanism restores `PlayerPosition` to a confirmed
/// checkpoint and then re-runs `FixedUpdate`.  Without this sync, physics
/// would re-simulate from the stale `Transform` instead of the rolled-back
/// position, causing divergence.
fn sync_position_to_transform(
    mut query: Query<
        (&PlayerPosition, &mut Transform),
        (With<NetworkCharacter>, With<Predicted>),
    >,
) {
    for (pos, mut transform) in &mut query {
        transform.translation = pos.to_vec3();
    }
}

/// Syncs the physics-resolved [`Transform::translation`] back to
/// [`PlayerPosition`] **after** physics finalises.
///
/// This keeps the prediction checkpoint (stored by lightyear in
/// `PlayerPosition`) up to date so rollbacks start from the correct position.
fn sync_transform_to_position(
    mut query: Query<
        (&Transform, &mut PlayerPosition),
        (With<NetworkCharacter>, With<Predicted>),
    >,
) {
    for (transform, mut pos) in &mut query {
        *pos = PlayerPosition::from_vec3(transform.translation);
    }
}

/// Applies the locally-controlled player's buffered [`PlayerInput`] to the
/// [`Predicted`] entity's [`CharacterController`].
///
/// Uses the same [`apply_input_to_controller`] function as the server so
/// rollback prediction is deterministic.
fn client_apply_inputs(
    mut query: Query<
        (&ActionState<PlayerInput>, &mut CharacterController),
        (With<NetworkCharacter>, With<Predicted>),
    >,
) {
    for (action, mut controller) in &mut query {
        apply_input_to_controller(action, &mut controller);
    }
}

/// Syncs the interpolated [`PlayerPosition`] to [`Transform`] for remote
/// player entities so they render at the smoothed position each frame.
///
/// This runs every frame in [`Update`] rather than `FixedUpdate` so rendering
/// is not locked to the fixed timestep.
fn sync_interpolated_position_to_transform(
    mut query: Query<
        (&PlayerPosition, &mut Transform),
        (With<NetworkCharacter>, With<Interpolated>),
    >,
) {
    for (pos, mut transform) in &mut query {
        transform.translation = pos.to_vec3();
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

/// Client-side character replication plugin.
///
/// Registered automatically by [`NetworkCharacterPlugin`] when the `client`
/// feature is active.
pub struct ClientCharacterPlugin;

impl Plugin for ClientCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_predicted_character_added);

        app.add_systems(
            FixedPreUpdate,
            buffer_client_input.in_set(InputSystems::WriteClientInputs),
        );

        app.add_systems(
            FixedUpdate,
            (
                // 1. Restore rolled-back PlayerPosition into Transform before physics.
                sync_position_to_transform,
                // 2. Write input intent into CharacterController before physics integrate.
                client_apply_inputs,
            )
                .before(PhysicsSet::Integrate),
        );

        app.add_systems(
            FixedUpdate,
            // 3. Write physics-resolved position back to PlayerPosition after finalise.
            sync_transform_to_position.after(PhysicsSet::Finalise),
        );

        app.add_systems(
            Update,
            // Runs every render frame so interpolated remote players are smooth.
            sync_interpolated_position_to_transform,
        );
    }
}
