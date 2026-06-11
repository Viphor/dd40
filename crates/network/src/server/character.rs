//! Server-side character replication systems.
//!
//! `ServerCharacterPlugin` is added automatically by `NetworkCharacterPlugin`
//! when the `server` feature is active.

use bevy::prelude::*;
use dd40_character_core::{builder::CharacterBuilder, controller::CharacterInput};
use dd40_identity_core::{Authenticated, PlayerIdentity, PlayerSpawnPosition};
use dd40_inventory_core::character_ext::CharacterInventoryExt;
use dd40_physics_core::character_ext::CharacterPhysicsExt;
use dd40_physics_core::prelude::{PhysicsPosition, PhysicsSet};
use lightyear::prelude::{RemoteId, input::native::ActionState, server::ClientOf};

use crate::character_ext::CharacterServerNetworkExt;
use crate::protocol::{NetworkCharacter, PlayerInput, PlayerPosition, PlayerRotation};
use crate::server::spawn::WorldSpawnConfig;
use crate::shared::character::apply_input_to_controller;

// ============================================================================
// OBSERVERS
// ============================================================================

/// Spawns a replicated character entity when a client has been authenticated.
///
/// Triggers on [`Authenticated`] (added by `dd40_identity` after JWT
/// verification), so characters only spawn for players whose identity has been
/// confirmed.
///
/// Spawn position comes from [`PlayerSpawnPosition`] on the connection entity
/// (set by `dd40_identity` when it loads the player's save file). Falls back
/// to [`WorldSpawnConfig::default_spawn`] for first-time connections.
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
    trigger: On<Add, Authenticated>,
    mut commands: Commands,
    client_query: Query<(&RemoteId, &PlayerIdentity, Option<&PlayerSpawnPosition>), With<ClientOf>>,
    spawn_config: Res<WorldSpawnConfig>,
) {
    let Ok((remote, identity, maybe_pos)) = client_query.get(trigger.entity) else {
        warn!(
            "Authenticated entity {:?} missing RemoteId/PlayerIdentity — skipping character spawn",
            trigger.entity
        );
        return;
    };
    let client_id = remote.0;

    let spawn_pos = maybe_pos
        .map(|p| p.0)
        .unwrap_or(spawn_config.default_spawn);

    info!(
        "Spawning network character for {} ({}) at {:?}",
        identity.display_name, identity.sub, spawn_pos
    );

    CharacterBuilder::new(identity.display_name.clone())
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
/// Registered automatically by `NetworkCharacterPlugin` when the `server`
/// feature is active.
pub struct ServerCharacterPlugin;

impl Plugin for ServerCharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(server_spawn_character);

        app.add_systems(
            FixedUpdate,
            server_apply_inputs.in_set(PhysicsSet::InputSync),
        );

        app.add_systems(FixedUpdate, server_sync_state.after(PhysicsSet::Finalise));
    }
}
