//! [`ConfigPlugin`] — loads all config layers and inserts Bevy Resources.

use bevy::prelude::*;

use crate::{
    ConfigDisk, RawConfig,
    env::apply_env_overrides,
    load::load_all,
};

/// Tier 0 plugin. Add **before** all other dd40 plugins.
///
/// Inserts [`RawConfig`] and (when a writable path is found) [`ConfigDisk`]
/// directly in [`Plugin::build`] so that other plugins added later can read
/// config values during their own `build` calls.
///
/// A [`PreStartup`] system logs the resolved table once the logging
/// subsystem is ready.
///
/// # Example
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use dd40_config::ConfigPlugin;
///
/// App::new()
///     .add_plugins(ConfigPlugin)
///     .run();
/// ```
#[derive(Default)]
pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        let (writable_path, mut merged) = load_all();

        // Capture the base (file-only) table before env overrides for delta-save.
        let base = merged.clone();

        apply_env_overrides(&mut merged);

        app.insert_resource(RawConfig(merged));

        if let Some(path) = writable_path {
            app.insert_resource(ConfigDisk { path, base });
        }

        // Log the resolved config once the logging subsystem is ready.
        app.add_systems(PreStartup, log_resolved_config);
    }
}

fn log_resolved_config(config: Res<RawConfig>) {
    debug!(config = ?config.0, "resolved config");
}
