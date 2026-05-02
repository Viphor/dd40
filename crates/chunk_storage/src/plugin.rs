use std::path::PathBuf;

use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;

use crate::{
    ChunkResponse, ChunkResponseReceiver, ChunkResponseSender, collect_chunk_responses,
    dispatch_chunk_requests, provider::DiskChunkProvider,
};

/// Bevy plugin that wires up file-based chunk storage.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_chunk_storage::plugin::DiskStoragePlugin;
///
/// fn main() {
///     App::new()
///         .add_plugins(MinimalPlugins)
///         .add_plugins(DiskStoragePlugin::new("world_data/chunks"))
///         .run();
/// }
/// ```
pub struct DiskStoragePlugin {
    pub dir: PathBuf,
}

impl DiskStoragePlugin {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl Plugin for DiskStoragePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);

        let (tx, rx) = crossbeam_channel::unbounded::<ChunkResponse>();
        app.insert_resource(ChunkResponseSender(tx));
        app.insert_resource(ChunkResponseReceiver(rx));
        app.insert_resource(DiskChunkProvider::new(self.dir.clone()));

        app.add_systems(
            PreUpdate,
            (dispatch_chunk_requests, collect_chunk_responses),
        );
    }
}
