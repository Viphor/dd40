//! Disk-backed chunk provider for the dd40 voxel engine.
//!
//! This crate wires [`dd40_core::ChunkProvider`] to the local filesystem.
//! Each chunk is stored as an individual binary file named `chunk_X_Z.bin`
//! inside a configurable directory.
//!
//! # Quick start
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_chunk_storage::plugin::DiskStoragePlugin;
//!
//! App::new()
//!     .add_plugins(DiskStoragePlugin::new("saves/chunks"))
//!     .run();
//! ```

use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use dd40_core::prelude::*;

use crate::provider::DiskChunkProvider;

pub mod plugin;
pub mod provider;
pub mod serialization;

// ---------------------------------------------------------------------------
// Channel newtypes
// ---------------------------------------------------------------------------

enum ChunkResponse {
    Loaded(Chunk),
    Request(ChunkPos),
}

/// Wraps the sender half of the chunk-response channel as a Bevy resource.
#[derive(Resource)]
struct ChunkResponseSender(crossbeam_channel::Sender<ChunkResponse>);

/// Wraps the receiver half of the chunk-response channel as a Bevy resource.
#[derive(Resource)]
struct ChunkResponseReceiver(crossbeam_channel::Receiver<ChunkResponse>);

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Reads [`RequestChunk`] events and fires off an async load via [`DiskChunkProvider`].
fn dispatch_chunk_requests(
    provider: Res<DiskChunkProvider>,
    mut requests: MessageReader<RequestChunk>,
    sender: Res<ChunkResponseSender>,
) {
    for event in requests.read() {
        provider.request(event.pos, sender.0.clone());
    }
}

/// Drains completed chunk loads from the background channel and emits
/// [`ChunkReady`] events so that [`dd40_core::apply_chunk_responses`] can
/// insert them into [`BlockStorage`].
fn collect_chunk_responses(
    receiver: Res<ChunkResponseReceiver>,
    mut ready: MessageWriter<ChunkReady>,
    mut requests: MessageWriter<GenerateChunk>,
) {
    while let Ok(message) = receiver.0.try_recv() {
        match message {
            ChunkResponse::Loaded(chunk) => {
                debug!(
                    "DiskStoragePlugin: loaded chunk at ({}, {})",
                    chunk.position().x,
                    chunk.position().z
                );
                ready.write(ChunkReady { chunk });
            }
            ChunkResponse::Request(pos) => {
                debug!(
                    "DiskStoragePlugin: no chunk found at ({}, {}), Generating new chunk",
                    pos.x, pos.z
                );
                requests.write(GenerateChunk { pos });
            }
        }
    }
}
