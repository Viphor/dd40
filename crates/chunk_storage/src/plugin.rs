use std::path::PathBuf;

use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;

use crate::{
    ChunkResponse, ChunkResponseReceiver, ChunkResponseSender, collect_chunk_responses,
    dispatch_chunk_requests, provider::DiskChunkProvider,
};

/// Environment variable that selects the on-disk chunk format written by
/// [`DiskStoragePlugin`].
///
/// Recognised truthy values (case-insensitive): `1`, `true`, `yes`, `on`.
/// Any other value — or no value at all — is treated as `false`. When
/// `true`, the writer persists each chunk's `confirmed_history` so the
/// server can serve delta updates after a restart.
pub const SAVE_HISTORY_ENV: &str = "DD40_CHUNK_STORAGE__SAVE_HISTORY";

fn parse_save_history_value(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn read_save_history_env() -> bool {
    match std::env::var(SAVE_HISTORY_ENV) {
        Ok(raw) => parse_save_history_value(&raw),
        Err(_) => false,
    }
}

/// Bevy plugin that wires up file-based chunk storage.
///
/// At plugin construction time the [`SAVE_HISTORY_ENV`] environment variable
/// is read once and used to choose the on-disk format. Use
/// [`DiskStoragePlugin::with_save_history`] to override the env var
/// programmatically (e.g. for tests).
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
    /// Whether the writer should persist chunk history. When `None`, the
    /// value is read from [`SAVE_HISTORY_ENV`] at plugin build time.
    pub save_history: Option<bool>,
}

impl DiskStoragePlugin {
    /// Creates a plugin that writes chunks under `dir`. The save format is
    /// chosen from [`SAVE_HISTORY_ENV`] at plugin build time.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            save_history: None,
        }
    }

    /// Creates a plugin with an explicit save-history setting that
    /// overrides the environment variable.
    pub fn with_save_history(dir: impl Into<PathBuf>, save_history: bool) -> Self {
        Self {
            dir: dir.into(),
            save_history: Some(save_history),
        }
    }
}

impl Plugin for DiskStoragePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);

        let save_history = self.save_history.unwrap_or_else(read_save_history_env);
        info!(
            "DiskStoragePlugin: dir = {}, save_history = {} (env {} = {:?})",
            self.dir.display(),
            save_history,
            SAVE_HISTORY_ENV,
            std::env::var(SAVE_HISTORY_ENV).ok(),
        );

        let (tx, rx) = crossbeam_channel::unbounded::<ChunkResponse>();
        app.insert_resource(ChunkResponseSender(tx));
        app.insert_resource(ChunkResponseReceiver(rx));
        app.insert_resource(DiskChunkProvider::with_history(
            self.dir.clone(),
            save_history,
        ));

        app.add_systems(
            PreUpdate,
            (dispatch_chunk_requests, collect_chunk_responses),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::parse_save_history_value;

    #[test]
    fn parse_truthy_values() {
        for v in ["1", "true", "TRUE", " yes ", "on", "Yes", "On"] {
            assert!(parse_save_history_value(v), "expected truthy: {v:?}");
        }
    }

    #[test]
    fn parse_falsy_values() {
        for v in ["", "0", "false", "no", "off", "anything", "2"] {
            assert!(!parse_save_history_value(v), "expected falsy: {v:?}");
        }
    }
}
