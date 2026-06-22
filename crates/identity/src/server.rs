use std::path::PathBuf;

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_identity_core::{
    AuthConfig, AuthTokenReceived, Authenticated, AwaitingAuth, IdentityCorePlugin,
    PlayerIdentity, PlayerSaveState, PlayerSpawnPosition,
};
use dd40_physics_core::prelude::PhysicsPosition;
use lightyear::prelude::server::ClientOf;
use lightyear_connection::client::Disconnecting;
use lightyear_connection::client_of::SkipNetcode;

use crate::access_list;
use crate::jwt::{self, JwksCache};
use crate::player_state;

/// Server-side identity plugin.
///
/// Reads [`AuthTokenReceived`] local messages (emitted by `dd40_network`'s
/// bridge), verifies the JWT, enforces access lists, and inserts
/// [`PlayerIdentity`] + [`Authenticated`] on the connection entity.
///
/// Also loads and saves per-player state ([`PlayerSaveState`]) keyed by
/// the `sub` claim.
pub struct IdentityServerPlugin {
    /// Directory where per-player save files are stored.
    ///
    /// Files are written as `<players_dir>/<sub>.bin`.
    pub players_dir: PathBuf,
}

impl IdentityServerPlugin {
    /// Create the plugin, storing player save files in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            players_dir: dir.into(),
        }
    }
}

impl Plugin for IdentityServerPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, IdentityCorePlugin);

        app.insert_resource(PlayersDir(self.players_dir.clone()));

        // Fetch JWKS at startup (blocking).
        app.add_systems(Startup, fetch_jwks_startup);

        // Verify incoming auth tokens each frame.
        app.add_systems(Update, verify_auth_tokens);

        // Disconnect connections that have been waiting too long without auth.
        app.add_systems(Update, timeout_unauthenticated);

        // Load player state after authentication.
        app.add_observer(on_authenticated);

        // Save player state when a connection loses its Authenticated marker.
        app.add_observer(on_authenticated_removed);
    }
}

// ============================================================================
// Resources
// ============================================================================

/// Directory path for per-player save files.
#[derive(Resource)]
struct PlayersDir(PathBuf);

// ============================================================================
// Systems
// ============================================================================

fn fetch_jwks_startup(config: Res<AuthConfig>, mut commands: Commands) {
    if config.jwks_uri.is_empty() {
        warn!("auth.jwks_uri is not configured — JWT verification will always fail");
        commands.insert_resource(JwksCache::default());
        return;
    }

    let keys = jwt::fetch_jwks(&config.jwks_uri);
    if keys.is_empty() {
        warn!(uri = %config.jwks_uri, "JWKS fetch returned no usable keys");
    } else {
        info!(count = keys.len(), uri = %config.jwks_uri, "JWKS keys loaded");
    }
    commands.insert_resource(JwksCache { keys });
}

fn verify_auth_tokens(
    mut reader: MessageReader<AuthTokenReceived>,
    config: Res<AuthConfig>,
    jwks: Res<JwksCache>,
    mut commands: Commands,
    connection_entities: Query<Entity>,
) {
    let allow_set = access_list::resolve(&config.allow);
    let deny_set = access_list::resolve(&config.deny);

    for msg in reader.read() {
        let entity = msg.connection_entity;

        // Entity may have already been despawned (race with timeout).
        if connection_entities.get(entity).is_err() {
            continue;
        }

        match jwt::verify(&msg.token, &jwks.keys, &config.issuer, config.audience.as_deref()) {
            Ok(claims) => {
                let sub = claims.sub.clone();
                let display_name = claims.preferred_username.unwrap_or_else(|| sub.clone());

                if !access_list::is_allowed(&sub, &allow_set, &deny_set) {
                    warn!(sub = %sub, "connection denied by access list");
                    commands
                        .entity(entity)
                        .remove::<AwaitingAuth>()
                        .insert((Disconnecting, SkipNetcode));
                    continue;
                }

                info!(sub = %sub, name = %display_name, "player authenticated");

                commands.entity(entity).remove::<AwaitingAuth>().insert((
                    PlayerIdentity { sub, display_name },
                    Authenticated,
                ));
            }
            Err(e) => {
                warn!(error = %e, "JWT verification failed — disconnecting client");
                commands
                    .entity(entity)
                    .remove::<AwaitingAuth>()
                    .insert((Disconnecting, SkipNetcode));
            }
        }
    }
}

fn timeout_unauthenticated(
    mut commands: Commands,
    // Only handle entities that have completed the netcode handshake (ClientOf).
    // Pre-handshake LinkOf-only entities are timed out by the netcode layer itself
    // (client_timeout_secs = 3 s) before our auth timeout can fire — no action needed.
    waiting: Query<(Entity, &AwaitingAuth), With<ClientOf>>,
    config: Res<AuthConfig>,
) {
    let timeout = std::time::Duration::from_secs(config.auth_timeout_secs);

    for (entity, awaiting) in &waiting {
        if awaiting.connected_at.elapsed() > timeout {
            warn!(
                entity = ?entity,
                timeout_secs = config.auth_timeout_secs,
                "auth timeout — dropping unauthenticated client"
            );
            // SkipNetcode removes the entity from both the netcode send and receive
            // queries so the server stops sending keepalives immediately.  The client
            // detects the missing keepalives after client_timeout_secs (~3 s) and
            // enters ConnectionTimedOut, which stops its own keepalive traffic.  The
            // netcode server's update_state then sees the silent link and fires
            // on_disconnect, which lets lightyear despawn the entity cleanly — no
            // manual despawn that would race with lightyear's connection-cache cleanup.
            commands
                .entity(entity)
                .remove::<AwaitingAuth>()
                .insert((Disconnecting, SkipNetcode));
        }
    }
}

// ============================================================================
// Observers
// ============================================================================

fn on_authenticated(
    trigger: On<Add, Authenticated>,
    identity_query: Query<&PlayerIdentity>,
    players_dir: Res<PlayersDir>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(identity) = identity_query.get(entity) else {
        return;
    };

    if let Some(state) = player_state::load(&players_dir.0, &identity.sub) {
        let pos: bevy::math::Vec3 = state.last_position.into();
        commands.entity(entity).insert(PlayerSpawnPosition(pos));
        debug!(sub = %identity.sub, pos = ?pos, "player state loaded");
    }
}

fn on_authenticated_removed(
    trigger: On<Remove, Authenticated>,
    identity_query: Query<&PlayerIdentity>,
    char_query: Query<(&PhysicsPosition, &lightyear::prelude::ControlledBy), With<ClientOf>>,
    players_dir: Res<PlayersDir>,
) {
    let connection_entity = trigger.entity;
    let Ok(identity) = identity_query.get(connection_entity) else {
        return;
    };

    let pos = char_query
        .iter()
        .find(|(_, cb)| cb.owner == connection_entity)
        .map(|(phys, _)| phys.0)
        .unwrap_or(bevy::math::Vec3::ZERO);

    let state = PlayerSaveState {
        last_position: pos.into(),
        inventory: vec![],
    };

    if let Err(e) = player_state::save(&players_dir.0, &identity.sub, &state) {
        warn!(sub = %identity.sub, error = %e, "failed to save player state on disconnect");
    } else {
        debug!(sub = %identity.sub, pos = ?pos, "player state saved");
    }
}
