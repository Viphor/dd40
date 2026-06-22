use bevy::prelude::*;

/// Stable OIDC-derived identity attached to every authenticated connection entity.
///
/// Added by `IdentityServerPlugin` after JWT verification succeeds. The `sub`
/// claim is the durable player identifier used for save-file lookups.
#[derive(Component, Clone, Debug)]
pub struct PlayerIdentity {
    /// OIDC subject claim — stable, opaque, unique per provider.
    pub sub: String,
    /// `preferred_username` claim — display name shown in-game.
    pub display_name: String,
}

/// Marker component on a connection entity indicating the player has been
/// verified and is allowed to interact with the world.
///
/// World-interaction systems should gate on `With<Authenticated>`.
/// Added by `IdentityServerPlugin` after JWT verification; removed on disconnect.
#[derive(Component, Clone, Debug, Default)]
pub struct Authenticated;

/// Marker component on a connection entity indicating we are waiting for the
/// client to present its `AuthToken`.
///
/// Added by the network bridge when a client connects; replaced by
/// [`Authenticated`] on success or removed on timeout/failure.
#[derive(Component, Clone, Debug)]
pub struct AwaitingAuth {
    /// Instant at which this connection was established, used to enforce the
    /// auth timeout.
    pub connected_at: std::time::Instant,
}

impl Default for AwaitingAuth {
    fn default() -> Self {
        Self {
            connected_at: std::time::Instant::now(),
        }
    }
}
