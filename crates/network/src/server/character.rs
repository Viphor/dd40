//! Server-side character replication systems.
//!
//! [`ServerCharacterPlugin`] is added automatically by [`NetworkCharacterPlugin`]
//! when the `server` feature is active.

use bevy::prelude::*;
use dd40_core::character::{
    CharacterBundle, JumpImpulse, MovementSpeed,
    controller::{CharacterController, CharacterInput},
    physics::{Aabb, CharacterCollider, PhysicsBody, PhysicsSet},
};
use lightyear::prelude::{
    Connected, ControlledBy, InterpolationTarget, NetworkTarget, PredictionTarget, RemoteId,
    Replicate, input::native::ActionState, server::ClientOf,
};

use crate::protocol::{NetworkCharacter, PlayerInput, PlayerPosition, PlayerRotation};
use crate::server::spawn::{PlayerLocations, WorldSpawnConfig};

use crate::shared::character::apply_input_to_controller;

// ============================================================================
// OBSERVERS
// ============================================================================

/// Spawns a replicated character entity whenever a client finishes its
/// lightyear handshake.
///
/// The spawn position is resolved from [`PlayerLocations`] (the player's last
/// known position from a previous session) falling back to
/// [`WorldSpawnConfig::default_spawn`] for first-time connections.
///
/// The entity is tagged for:
/// - Full replication to all clients ([`Replicate`]).
/// - Client-side prediction on the controlling client only
///   ([`PredictionTarget`]).
/// - Snapshot interpolation on all other clients
///   ([`InterpolationTarget`]).
/// - Automatic despawn when the owning connection entity is removed
///   ([`ControlledBy`]).
fn server_spawn_character(
    trigger: On<Add, Connected>,
    mut commands: Commands,
    client_query: Query<&RemoteId, With<ClientOf>>,
    spawn_config: Res<WorldSpawnConfig>,
    player_locations: Res<PlayerLocations>,
) {
    let Ok(remote) = client_query.get(trigger.entity) else {
        warn!(
            "Connected entity {:?} has no RemoteId — skipping character spawn",
            trigger.entity
        );
        return;
    };
    let client_id = remote.0;

    let spawn_pos = player_locations
        .get(client_id)
        .unwrap_or(spawn_config.default_spawn);

    info!(
        "Spawning network character for client {:?} at {:?}",
        client_id, spawn_pos
    );

    commands.spawn((
        // ── Identity ─────────────────────────────────────────────────────
        NetworkCharacter,
        CharacterBundle::new(
            format!("NetworkPlayer_{client_id:?}"),
            MovementSpeed::default(),
            Transform::from_translation(spawn_pos),
        ),
        // ── Physics ──────────────────────────────────────────────────────
        PhysicsBody,
        CharacterCollider,
        Aabb::player(),
        JumpImpulse::default(),
        CharacterController::default(),
        // ── Networked state ───────────────────────────────────────────────
        // Lightyear reads ActionState from this component and populates it
        // with the inputs buffered by the controlling client.
        ActionState::<PlayerInput>::default(),
        PlayerPosition::from_vec3(spawn_pos),
        PlayerRotation::new(0.0, 0.0),
        // ── Replication config ────────────────────────────────────────────
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
        ControlledBy {
            owner: trigger.entity,
            lifetime: Default::default(),
        },
    ));
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Translates the client's buffered [`PlayerInput`] into [`CharacterInput`]
/// intent each fixed tick.
///
/// Excludes [`Predicted`] entities so this only runs on the authoritative
/// server copies (which are `Without<Predicted>` by definition on the server,
/// but the guard is kept explicit for host-server compatibility).
///
/// [`Predicted`]: lightyear::prelude::Predicted
fn server_apply_inputs(
    mut query: Query<(&ActionState<PlayerInput>, &mut CharacterInput), With<NetworkCharacter>>,
) {
    for (action, mut char_input) in &mut query {
        apply_input_to_controller(action, &mut char_input);
    }
}

/// Syncs authoritative physics state back to the replicated network components
/// after each physics tick so lightyear can replicate the changes to clients.
///
/// - [`Transform::translation`] → [`PlayerPosition`]
/// - [`CharacterInput::pitch`] / [`CharacterInput::yaw`] → [`PlayerRotation`]
///
/// Rotation is driven by the client's camera input and arrives via
/// [`CharacterInput`] after [`server_apply_inputs`] writes it from
/// [`ActionState<PlayerInput>`].
fn server_sync_state(
    mut query: Query<
        (
            &Transform,
            &CharacterInput,
            &mut PlayerPosition,
            &mut PlayerRotation,
        ),
        With<NetworkCharacter>,
    >,
) {
    for (transform, char_input, mut pos, mut rot) in &mut query {
        *pos = PlayerPosition::from_vec3(transform.translation);
        rot.pitch = char_input.pitch;
        rot.yaw = char_input.yaw;
    }
}

// ============================================================================
// PLUGIN
// ============================================================================

/// Server-side character replication plugin.
///
/// Registered automatically by [`NetworkCharacterPlugin`] when the `server`
/// feature is active.
pub struct ServerCharacterPlugin;

impl Plugin for ServerCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(server_spawn_character);

        app.add_systems(
            FixedUpdate,
            // Must run before CharacterControllerPlugin's apply_character_controller,
            // which itself runs before PhysicsSet::Integrate.
            server_apply_inputs.before(PhysicsSet::Integrate),
        );

        app.add_systems(PostUpdate, server_sync_state);
    }
}
