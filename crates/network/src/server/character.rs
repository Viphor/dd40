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

    let character = CharacterBuilder::new(user.name)
        .transform(Transform::from_translation(spawn_pos))
        .with_physics()
        .with_controller()
        .with_inventory(36)
        .with_server_replication(client_id, spawn_pos, trigger.entity)
        .spawn(&mut commands)
        .id();

    spawn_on_foot_action_entities_server(&mut commands, character, client_id);
}

/// Spawns the per-character `OnFoot` BEI action entities authoritatively
/// on the server and configures them to replicate to the owning client.
///
/// lightyear's input pipeline expects each `Action<T>` entity to exist
/// on both the server and the client and to be linked by a deterministic
/// [`PreSpawned`] hash. The server is the authority for the player
/// entity, so it must own the action entities too — the client only
/// spawns local mirrors keyed by the same hash (see
/// `dd40_player_input::bindings::spawn_local_player_input_tree`).
///
/// We avoid having lightyear spawn a Predicted / Interpolated copy of
/// these entities on the client by setting both [`PredictionTarget`]
/// and [`InterpolationTarget`] to empty manual targets — the prespawn
/// pairing alone is sufficient.
fn spawn_on_foot_action_entities_server(
    commands: &mut Commands,
    character: Entity,
    client_id: lightyear::prelude::PeerId,
) {
    use dd40_input_core::actions::{
        Attack, CameraRotation, Interact, Jump, Move, Place, Sprint,
    };
    use dd40_input_core::contexts::OnFoot;
    use dd40_input_core::prespawn::{OnFootAction, on_foot_action_prespawn_hash};
    use lightyear::input::bei::prelude::{Action, ActionOf};
    use lightyear::prelude::{
        InterpolationTarget, NetworkTarget, PreSpawned, PredictionTarget, Replicate,
    };

    let client_bits = client_id.to_bits();
    let target = NetworkTarget::Single(client_id);

    fn server_bundle(
        client_bits: u64,
        action: OnFootAction,
        target: NetworkTarget,
    ) -> (
        PreSpawned,
        Replicate,
        PredictionTarget,
        InterpolationTarget,
    ) {
        (
            PreSpawned::new(on_foot_action_prespawn_hash(client_bits, action)),
            Replicate::to_clients(target),
            PredictionTarget::manual(Vec::new()),
            InterpolationTarget::manual(Vec::new()),
        )
    }

    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Move>::new(),
        server_bundle(client_bits, OnFootAction::Move, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Jump>::new(),
        server_bundle(client_bits, OnFootAction::Jump, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Sprint>::new(),
        server_bundle(client_bits, OnFootAction::Sprint, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Attack>::new(),
        server_bundle(client_bits, OnFootAction::Attack, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Place>::new(),
        server_bundle(client_bits, OnFootAction::Place, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<Interact>::new(),
        server_bundle(client_bits, OnFootAction::Interact, target.clone()),
    ));
    commands.spawn((
        ActionOf::<OnFoot>::new(character),
        Action::<CameraRotation>::new(),
        server_bundle(client_bits, OnFootAction::CameraRotation, target),
    ));
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
