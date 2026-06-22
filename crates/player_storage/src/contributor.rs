use std::sync::Arc;

use bevy::prelude::*;

/// Implementors serialise and deserialise one logical chunk of player state
/// (e.g. inventory, quest flags, equipped items).
///
/// Each contributor owns a stable `kind` key that identifies its blob in the
/// save file, and a `current_version` for independent format evolution.
/// `player_storage` writes `[u16 LE version][payload]` into every blob; the
/// version is split back out before `load` is called, so contributors can
/// branch on it for migrations.
pub trait PlayerStateContributor: Send + Sync + 'static {
    /// Stable, unique key stored in the save file.
    fn kind(&self) -> &'static str;

    /// Current serialisation version for this contributor's format.
    fn current_version(&self) -> u16;

    /// Serialise component(s) from `entity`. Return empty `Vec` if absent.
    fn save(&self, entity: &EntityRef) -> Vec<u8>;

    /// Queue component insertion on the just-spawned character entity.
    ///
    /// `version` is the version that was written; branch on it for migrations.
    fn load(&self, entity: Entity, version: u16, data: &[u8], commands: &mut Commands);
}

/// Runtime registry of [`PlayerStateContributor`]s.
///
/// Inserted as a resource by [`PlayerStoragePlugin`]. Contributor crates
/// register themselves during their plugin's `build` after calling
/// `ensure_plugins!(app, PlayerStoragePlugin)`.
#[derive(Resource, Default)]
pub struct PlayerStateRegistry {
    contributors: Vec<Arc<dyn PlayerStateContributor>>,
}

impl PlayerStateRegistry {
    /// Register `contributor`. Silently ignored if a contributor with the same
    /// `kind()` is already registered.
    pub fn register(&mut self, contributor: impl PlayerStateContributor) {
        let kind = contributor.kind();
        if self.contributors.iter().any(|c| c.kind() == kind) {
            return;
        }
        self.contributors.push(Arc::new(contributor));
    }

    /// All registered contributors in registration order.
    pub fn contributors(&self) -> &[Arc<dyn PlayerStateContributor>] {
        &self.contributors
    }

    /// Find a contributor by kind key.
    pub fn find(&self, kind: &str) -> Option<&Arc<dyn PlayerStateContributor>> {
        self.contributors.iter().find(|c| c.kind() == kind)
    }
}

/// Transient blobs from a loaded save, placed on the connection entity after
/// authentication and consumed by `server_spawn_character`.
///
/// Each entry is `(contributor kind, versioned blob)` where the blob is
/// `[u16 LE version][payload]` as written by [`PlayerStateRegistry`] helpers.
#[derive(Component, Default)]
pub struct PlayerSavedStateBlobs(pub Vec<(String, Vec<u8>)>);
