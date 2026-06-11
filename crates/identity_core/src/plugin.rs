use bevy::prelude::*;
use dd40_config::{ConfigPlugin, RawConfig};
use dd40_core::ensure_plugins;

use crate::{AuthConfig, AuthTokenReceived};

/// Foundation plugin for the identity system.
///
/// Registers the [`AuthConfig`] resource and the local [`AuthTokenReceived`]
/// message type. Must be added before any plugin that reads `AuthConfig`.
///
/// Added automatically via [`ensure_plugins!`] in `IdentityServerPlugin` and
/// `IdentityClientPlugin`.
#[derive(Default)]
pub struct IdentityCorePlugin;

impl Plugin for IdentityCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, ConfigPlugin);
        app.add_message::<AuthTokenReceived>();
        app.add_systems(Startup, insert_auth_config);
    }
}

fn insert_auth_config(raw: Option<Res<RawConfig>>, mut commands: Commands) {
    let cfg = raw
        .as_ref()
        .map(|r| r.section::<AuthConfig>())
        .unwrap_or_default();
    commands.insert_resource(cfg);
}
