use std::path::PathBuf;

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_identity_core::{
    AuthConfig, AuthTokenReceived, Authenticated, AwaitingAuth, IdentityCorePlugin, PlayerIdentity,
};
use dd40_player_storage::PlayersDir;
use lightyear::prelude::server::ClientOf;
use lightyear_connection::client::Disconnecting;
use lightyear_connection::client_of::SkipNetcode;

use crate::access_list;
use crate::jwt::{self, JwksCache};

/// Server-side identity plugin.
///
/// Verifies JWT tokens presented by connecting clients, enforces access lists,
/// and inserts [`PlayerIdentity`] + [`Authenticated`] on the connection entity.
///
/// Also inserts [`PlayersDir`] so that `dd40_network` can locate per-player
/// save files when it loads and saves player state.
pub struct IdentityServerPlugin {
    /// Directory where per-player save files are stored.
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

        app.add_systems(Startup, fetch_jwks_startup);
        app.add_systems(Update, verify_auth_tokens);
        app.add_systems(Update, timeout_unauthenticated);
    }
}

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
            commands
                .entity(entity)
                .remove::<AwaitingAuth>()
                .insert((Disconnecting, SkipNetcode));
        }
    }
}
