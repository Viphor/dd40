use bevy::prelude::*;
use dd40_config::{ConfigPlugin, RegisterConfig};
use dd40_core::ensure_plugins;

use crate::{AuthConfig, AuthTokenReceived};

/// Foundation plugin for the identity system.
///
/// Inserts [`AuthConfig`] as a resource (eagerly, during `Plugin::build`) and
/// registers the local [`AuthTokenReceived`] message type.
///
/// Because the resource is inserted before any system runs, every `Startup`
/// system can safely read `Res<AuthConfig>` without ordering constraints.
///
/// Added automatically via [`ensure_plugins!`] in `IdentityServerPlugin` and
/// `IdentityClientPlugin`.
#[derive(Default)]
pub struct IdentityCorePlugin;

impl Plugin for IdentityCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, ConfigPlugin);
        app.register_config::<AuthConfig>();
        app.add_message::<AuthTokenReceived>();
    }
}
