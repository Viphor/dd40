use std::path::PathBuf;

use bevy::prelude::*;
use dd40_config::{ConfigSection, RawConfig};
use dd40_core::block::BlockDataTypeRegistry;
use dd40_core::plugin::CorePlugin;
use serde::{Deserialize, Serialize};

use crate::{
    ChunkResponse, ChunkResponseReceiver, ChunkResponseSender,
    chunk_save_on_exit::save_chunks_on_exit,
    collect_chunk_responses, dispatch_chunk_requests,
    entity_persistence::{EntityPersistenceConfig, load_entities_for_ready_chunks, save_entities_on_exit},
    provider::DiskChunkProvider,
};

/// Config section for `dd40_chunk_storage`.
///
/// Read from the `[chunk_storage]` table in `config.toml` and overridable
/// via `DD40_CHUNK_STORAGE__*` environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChunkStorageConfig {
    /// Persist `confirmed_history` alongside block data. Required for
    /// the server to serve delta updates after a restart.
    /// Default: `false`.
    pub save_history: bool,
    /// Persist per-chunk entity sidecars (loose items, etc.).
    /// Default: `true`.
    pub save_entities: bool,
}

impl Default for ChunkStorageConfig {
    fn default() -> Self {
        Self {
            save_history: false,
            save_entities: true,
        }
    }
}

impl ConfigSection for ChunkStorageConfig {
    const SECTION: &'static str = "chunk_storage";
}

/// Bevy plugin that wires up file-based chunk storage.
///
/// Config is resolved in priority order (highest wins):
/// 1. Values passed via [`DiskStoragePlugin::with_save_history`] /
///    [`DiskStoragePlugin::with_save_entities`] (for tests / explicit overrides).
/// 2. The `[chunk_storage]` section of `config.toml` as loaded by
///    [`dd40_config::ConfigPlugin`] (including `DD40_CHUNK_STORAGE__*` env var
///    overrides applied by that plugin).
/// 3. Compiled-in defaults (`save_history = false`, `save_entities = true`).
///
/// [`dd40_config::ConfigPlugin`] must be added before this plugin for file /
/// env-var config to take effect.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_chunk_storage::plugin::DiskStoragePlugin;
///
/// App::new()
///     .add_plugins(MinimalPlugins)
///     .add_plugins(DiskStoragePlugin::new("world_data/chunks"))
///     .run();
/// ```
pub struct DiskStoragePlugin {
    pub dir: PathBuf,
    /// Explicit override for `save_history`. When `None`, the value comes
    /// from `RawConfig` (or the compiled-in default).
    pub save_history: Option<bool>,
    /// Explicit override for `save_entities`. When `None`, the value comes
    /// from `RawConfig` (or the compiled-in default).
    pub save_entities: Option<bool>,
}

impl DiskStoragePlugin {
    /// Creates a plugin that writes chunks under `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            save_history: None,
            save_entities: None,
        }
    }

    /// Creates a plugin with an explicit `save_history` setting that
    /// overrides config and env vars (useful in tests).
    pub fn with_save_history(dir: impl Into<PathBuf>, save_history: bool) -> Self {
        Self {
            dir: dir.into(),
            save_history: Some(save_history),
            save_entities: None,
        }
    }

    /// Overrides the entity-sidecar toggle (useful in tests).
    pub fn with_save_entities(mut self, save_entities: bool) -> Self {
        self.save_entities = Some(save_entities);
        self
    }
}

impl Plugin for DiskStoragePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);

        let cfg = app
            .world()
            .get_resource::<RawConfig>()
            .map(|r| r.section::<ChunkStorageConfig>())
            .unwrap_or_default();

        let save_history = self.save_history.unwrap_or(cfg.save_history);
        let save_entities = self.save_entities.unwrap_or(cfg.save_entities);

        info!(
            dir = %self.dir.display(),
            save_history,
            save_entities,
            "DiskStoragePlugin initialised"
        );

        let (tx, rx) = crossbeam_channel::unbounded::<ChunkResponse>();
        app.insert_resource(ChunkResponseSender(tx));
        app.insert_resource(ChunkResponseReceiver(rx));
        app.insert_resource(DiskChunkProvider::with_history(
            self.dir.clone(),
            save_history,
        ));
        app.insert_resource(EntityPersistenceConfig {
            enabled: save_entities,
            dir: self.dir.clone(),
        });

        app.add_systems(
            PreUpdate,
            (dispatch_chunk_requests, collect_chunk_responses),
        );

        // Sidecar load runs after collect_chunk_responses so any
        // ChunkReady written this frame is visible to the loader.
        app.add_systems(
            PreUpdate,
            load_entities_for_ready_chunks.after(collect_chunk_responses),
        );
        app.add_systems(Last, save_entities_on_exit);
        app.add_systems(Last, save_chunks_on_exit);

        // Snapshot the live BlockDataTypeRegistry into the disk provider
        // once startup finishes — at that point every plugin has had a
        // chance to call `register_block_data::<T>()`.
        app.add_systems(Startup, snapshot_registry_into_provider);
    }
}

/// Copies the current [`BlockDataTypeRegistry`] into the
/// [`DiskChunkProvider`] so the background load thread has the
/// type-decoder table it needs to decode cell-data entries on load.
fn snapshot_registry_into_provider(
    registry: Res<BlockDataTypeRegistry>,
    mut provider: ResMut<DiskChunkProvider>,
) {
    provider.set_registry(registry.clone());
}
