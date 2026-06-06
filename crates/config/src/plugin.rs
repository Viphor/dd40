//! [`ConfigPlugin`] — loads all config layers and inserts Bevy Resources.

use bevy::prelude::*;

use crate::{
    ConfigDisk, RawConfig,
    env::apply_env_overrides,
    load::load_all,
};

/// Tier 0 plugin. Add **before** all other dd40 plugins.
///
/// Runs in [`PreStartup`] so every `Startup` system can read [`RawConfig`].
///
/// Inserts:
/// - [`RawConfig`] — the fully-merged config table (files + env vars).
/// - [`ConfigDisk`] — the writable save target (if one could be determined).
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
        app.add_systems(PreStartup, load_and_insert_config);
    }
}

fn load_and_insert_config(mut commands: Commands) {
    let (writable_path, mut merged) = load_all();

    // Capture the base (file-only) table before env overrides for delta-save.
    let base = merged.clone();

    apply_env_overrides(&mut merged);

    debug!(config = ?merged, "loaded config");

    commands.insert_resource(RawConfig(merged));

    if let Some(path) = writable_path {
        commands.insert_resource(ConfigDisk { path, base });
    }
}
