use std::path::PathBuf;

use bevy::prelude::*;

use crate::contributor::PlayerStateRegistry;

/// Directory where per-player save files are stored.
///
/// Must be inserted as a resource before any save or load operations run.
/// Typically set by the binary or by the identity plugin at startup.
#[derive(Resource, Clone, Debug)]
pub struct PlayersDir(pub PathBuf);

/// Sets up the [`PlayerStateRegistry`] resource.
///
/// Contributor crates call `ensure_plugins!(app, PlayerStoragePlugin)` before
/// registering themselves into the registry.
#[derive(Default)]
pub struct PlayerStoragePlugin;

impl Plugin for PlayerStoragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerStateRegistry>();
    }
}
