//! Server-side character lifecycle systems.
//!
//! `ServerCharacterPlugin` is added automatically by `NetworkCharacterPlugin`
//! when the `server` feature is active.

use bevy::prelude::*;
use dd40_character_core::{builder::CharacterBuilder, controller::CharacterInput};
use dd40_identity_core::{Authenticated, PlayerIdentity};
use dd40_inventory_core::character_ext::CharacterInventoryExt;
use dd40_physics_core::character_ext::CharacterPhysicsExt;
use dd40_physics_core::prelude::{PhysicsPosition, PhysicsSet};
use dd40_player_storage::{
    load_player_state, save_player_state, PlayersDir, PlayerSaveState, PlayerStateRegistry,
};
use lightyear::prelude::{RemoteId, input::native::ActionState};

use crate::character_ext::CharacterServerNetworkExt;
use crate::protocol::{NetworkCharacter, PlayerInput, PlayerPosition, PlayerRotation};
use crate::server::spawn::WorldSpawnConfig;
use crate::shared::character::apply_input_to_controller;

// ============================================================================
// OBSERVERS
// ============================================================================

/// Loads the player's saved state, spawns their character at the saved
/// position, and applies any persisted component blobs (inventory, etc.)
/// via the [`PlayerStateRegistry`].
///
/// Triggers on [`Authenticated`] (added by `dd40_identity` after JWT
/// verification).
fn on_player_authenticated(
    trigger: On<Add, Authenticated>,
    mut commands: Commands,
    client_query: Query<(&RemoteId, &PlayerIdentity), With<lightyear::prelude::server::ClientOf>>,
    players_dir: Res<PlayersDir>,
    registry: Res<PlayerStateRegistry>,
    spawn_config: Res<WorldSpawnConfig>,
) {
    let connection_entity = trigger.entity;
    let Ok((remote, identity)) = client_query.get(connection_entity) else {
        warn!(
            "Authenticated entity {:?} missing RemoteId/PlayerIdentity — skipping character spawn",
            connection_entity
        );
        return;
    };
    let client_id = remote.0;

    let state = load_player_state(&players_dir.0, &identity.sub).unwrap_or_default();

    let saved_pos: Vec3 = state.last_position.into();
    let spawn_pos = if saved_pos == Vec3::ZERO {
        spawn_config.default_spawn
    } else {
        saved_pos
    };

    info!(
        sub = %identity.sub,
        name = %identity.display_name,
        pos = ?spawn_pos,
        "spawning character for authenticated player"
    );

    let char_entity = CharacterBuilder::new(identity.display_name.clone())
        .transform(Transform::from_translation(spawn_pos))
        .with_physics()
        .with_controller()
        .with_inventory(36)
        .with_server_replication(client_id, spawn_pos, connection_entity)
        .spawn(&mut commands)
        .id();

    // Apply each saved blob through its contributor.
    for (kind, versioned_data) in &state.blobs {
        if versioned_data.len() < 2 {
            warn!(kind = %kind, "saved blob too short to contain version prefix; skipping");
            continue;
        }
        let version = u16::from_le_bytes([versioned_data[0], versioned_data[1]]);
        if let Some(contributor) = registry.find(kind) {
            contributor.load(char_entity, version, &versioned_data[2..], &mut commands);
        } else {
            warn!(kind = %kind, "no contributor registered for saved blob kind; skipping");
        }
    }
}

/// Saves the player's position and all contributor blobs to disk when their
/// character entity is despawned.
///
/// Triggers on `Remove<ControlledBy>` filtered to [`NetworkCharacter`]
/// entities so it fires while the character entity is still fully populated —
/// lightyear despawns character entities *before* finishing the connection
/// entity teardown, meaning `Remove<Authenticated>` fires too late.
fn on_character_despawned(
    trigger: On<Remove, lightyear::prelude::ControlledBy>,
    // EntityRef gives synchronous read-only access to all components while
    // they are still present (Remove observers fire before the component
    // is actually removed).
    entity_query: Query<EntityRef, With<NetworkCharacter>>,
    identity_query: Query<&PlayerIdentity>,
    players_dir: Res<PlayersDir>,
    registry: Res<PlayerStateRegistry>,
) {
    let char_entity = trigger.entity;

    let Ok(entity_ref) = entity_query.get(char_entity) else {
        return;
    };

    let Some(controlled_by) = entity_ref.get::<lightyear::prelude::ControlledBy>() else {
        return;
    };

    let Ok(identity) = identity_query.get(controlled_by.owner) else {
        return;
    };

    let pos = entity_ref
        .get::<PhysicsPosition>()
        .map(|p| p.0)
        .unwrap_or(Vec3::ZERO);

    let blobs: Vec<(String, Vec<u8>)> = registry
        .contributors()
        .iter()
        .map(|c| {
            let payload = c.save(&entity_ref);
            let mut versioned = Vec::with_capacity(2 + payload.len());
            versioned.extend_from_slice(&c.current_version().to_le_bytes());
            versioned.extend_from_slice(&payload);
            (c.kind().to_string(), versioned)
        })
        .collect();

    let state = PlayerSaveState {
        last_position: pos.into(),
        blobs,
    };

    if let Err(e) = save_player_state(&players_dir.0, &identity.sub, &state) {
        warn!(sub = %identity.sub, error = %e, "failed to save player state on disconnect");
    } else {
        debug!(sub = %identity.sub, pos = ?pos, "player state saved");
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Translates the client's buffered [`PlayerInput`] into [`CharacterInput`]
/// intent each fixed tick.
fn server_apply_inputs(
    mut query: Query<(&ActionState<PlayerInput>, &mut CharacterInput), With<NetworkCharacter>>,
) {
    for (action, mut char_input) in &mut query {
        apply_input_to_controller(action, &mut char_input);
    }
}

/// Syncs authoritative physics state back to the replicated network components
/// after each physics tick.
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
        app.add_observer(on_player_authenticated);
        app.add_observer(on_character_despawned);

        app.add_systems(
            FixedUpdate,
            server_apply_inputs.in_set(PhysicsSet::InputSync),
        );

        app.add_systems(FixedUpdate, server_sync_state.after(PhysicsSet::Finalise));
    }
}
