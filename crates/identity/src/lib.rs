//! OIDC-based player identity for dd40.
//!
//! Provides two plugins:
//! - [`IdentityServerPlugin`]: JWT verification, access-list gating,
//!   and per-player save-state I/O. Add to `dd40_server`.
//! - [`IdentityClientPlugin`]: No-op anchor for future client-side auth
//!   logic. Add to `dd40_client`.

pub mod access_list;
pub mod jwt;
pub mod player_state;

mod server;

pub use server::IdentityServerPlugin;

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_identity_core::IdentityCorePlugin;

/// Client-side identity plugin.
///
/// Currently a no-op: the token-reading and sending logic lives in
/// `dd40_network`'s `send_auth_token` system. This plugin exists as a stable
/// anchor for future client-side auth behaviour (e.g. token refresh warnings).
#[derive(Default)]
pub struct IdentityClientPlugin;

impl Plugin for IdentityClientPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, IdentityCorePlugin);
    }
}
