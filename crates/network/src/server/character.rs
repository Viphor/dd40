//! Server-side character replication systems.
//!
//! [`ServerCharacterPlugin`] is added automatically by [`NetworkCharacterPlugin`]
//! when the `server` feature is active.

use bevy::prelude::*;
use dd40_character_core::{builder::CharacterBuilder, controller::CharacterInput};
use dd40_inventory_core::character_ext::CharacterInventoryExt;
use dd40_physics_core::character_ext::CharacterPhysicsExt;
use dd40_physics_core::prelude::{PhysicsPosition, PhysicsSet};
use lightyear::prelude::{Connected, RemoteId, server::ClientOf};

use crate::character_ext::CharacterServerNetworkExt;
use crate::protocol::{NetworkCharacter, PlayerPosition, PlayerRotation};
use crate::server::spawn::{PlayerLocations, WorldSpawnConfig};
use crate::server::user::get_user;

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

    let Some(user) = get_user(client_id.to_bits()) else {
        warn!(
            "No user found for client {:?} — skipping character spawn",
            client_id
        );
        return;
    };

    let spawn_pos = player_locations
        .get(client_id)
        .unwrap_or(spawn_config.default_spawn);

    info!(
        "Spawning network character for client {:?} at {:?}",
        client_id, spawn_pos
    );

    CharacterBuilder::new(user.name)
        .transform(Transform::from_translation(spawn_pos))
        .with_physics()
        .with_controller()
        .with_inventory(36)
        .with_server_replication(client_id, spawn_pos, trigger.entity)
        .spawn(&mut commands);
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Syncs authoritative physics state back to the replicated network components
/// after each physics tick so lightyear can replicate the changes to clients.
///
/// - [`Transform::translation`] → [`PlayerPosition`]
/// - [`CharacterInput::pitch`] / [`CharacterInput::yaw`] → [`PlayerRotation`]
///
/// Rotation is driven by the client's camera input and arrives via
/// `CharacterInput` after `PlayerInputTranslationPlugin` (in
/// `dd40_player_input`) copies the replicated `CameraRotation` action into
/// pitch / yaw each `FixedPreUpdate`.
fn server_sync_state(
    mut query: Query<
        (
            &PhysicsPosition,
            &CharacterInput,
            &mut PlayerPosition,
            &mut PlayerRotation,
        ),
        With<NetworkCharacter>,
    >,
) {
    for (char_pos, char_input, mut pos, mut rot) in &mut query {
        *pos = PlayerPosition::from_vec3(char_pos.0);
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
///
/// Action → `CharacterInput` translation is handled by
/// `PlayerInputTranslationPlugin` (added separately by the server binary),
/// not by this plugin — keeping the dependency graph one-directional
/// (network is pure transport; input semantics live in `dd40_player_input`).
pub struct ServerCharacterPlugin;

impl Plugin for ServerCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(server_spawn_character);

        app.add_systems(FixedUpdate, server_sync_state.after(PhysicsSet::Finalise));
    }
}
