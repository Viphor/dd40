use std::{
    io::{BufReader, Error, ErrorKind},
    path::PathBuf,
};

use bevy::prelude::*;
use dd40_core::prelude::*;

use crate::{
    ChunkResponse,
    serialization::{ChunkSerializeError, ChunkSerializeExt, deserialize_chunk},
};

/// A [`ChunkProvider`] that reads and writes chunks as bincode files on disk.
///
/// Each chunk is stored at `<dir>/chunk_<x>_<z>.bin`.
/// Loading is performed on a background thread so the main thread is never blocked.
#[derive(Resource)]
pub struct DiskChunkProvider {
    dir: PathBuf,
}

impl DiskChunkProvider {
    /// Creates a new provider that reads/writes chunks inside `dir`.
    ///
    /// The directory does **not** need to exist yet; it will be created on
    /// the first [`save`](DiskChunkProvider::save) call.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Returns the canonical file path for a chunk.
    ///
    /// Includes the Y axis (always `0` today) so that an eventual switch
    /// to vertically-split chunks does not invalidate save folders.
    fn chunk_path(&self, pos: ChunkPos) -> std::path::PathBuf {
        self.dir
            .join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z))
    }

    /// Synchronously saves `chunk` to the configured directory.
    /// Safe to call from any Bevy system that has `Res<DiskChunkProvider>`.
    pub fn save(&self, chunk: &Chunk) -> std::io::Result<()> {
        let path = self.chunk_path(chunk.position());
        chunk.save(&path).map_err(|e| match e {
            ChunkSerializeError::Io(io_err) => io_err,
            other => Error::new(ErrorKind::Other, other.to_string()),
        })
    }

    pub(crate) fn request(&self, pos: ChunkPos, sender: crossbeam_channel::Sender<ChunkResponse>) {
        let path = self.chunk_path(pos);
        // Spawn a background thread so disk I/O never blocks the main thread.
        std::thread::spawn(move || {
            let result = deserialize_chunk(BufReader::new(match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    let _ = sender.send(ChunkResponse::Request(pos));
                    return;
                }
                Err(e) => {
                    warn!("DiskChunkProvider: failed to open {:?}: {}", path, e);
                    let _ = sender.send(ChunkResponse::Request(pos));
                    return;
                }
            }));
            match result {
                Ok(chunk) => {
                    let _ = sender.send(ChunkResponse::Loaded(chunk));
                }
                Err(e) => {
                    warn!("DiskChunkProvider: failed to deserialize {:?}: {}", path, e);
                    let _ = sender.send(ChunkResponse::Request(pos));
                }
            }
        });
    }
}
