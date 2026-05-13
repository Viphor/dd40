use std::{
    io::{BufReader, Error, ErrorKind},
    path::PathBuf,
};

use bevy::prelude::*;
use dd40_core::prelude::*;

use crate::{
    ChunkResponse,
    serialization::{
        ChunkSerializeError, ChunkVersion, deserialize_chunk, serialize_chunk_versioned,
    },
};

/// A [`ChunkProvider`] that reads and writes chunks as binary files on disk.
///
/// Each chunk is stored at `<dir>/chunk_<x>_<y>_<z>.bin`.
/// Loading is performed on a background thread so the main thread is never
/// blocked.
///
/// # On-disk format selection
///
/// The reader auto-detects the format from the file header, so any version
/// supported by [`crate::serialization`] can be loaded transparently. The
/// writer's format is fixed at construction time:
///
/// - `save_history = false` (default) — writes [`ChunkVersion::V1`].
///   Smallest file. The chunk's confirmed history is dropped at save time.
/// - `save_history = true` — writes [`ChunkVersion::V1Versioned`]. The
///   chunk's confirmed history is persisted so the server can serve delta
///   updates after a restart.
///
/// See [`crate::plugin::DiskStoragePlugin`] for how the flag is wired from
/// the `DD40_CHUNK_STORAGE__SAVE_HISTORY` environment variable.
#[derive(Resource)]
pub struct DiskChunkProvider {
    dir: PathBuf,
    save_history: bool,
}

impl DiskChunkProvider {
    /// Creates a provider that writes the smallest format ([`ChunkVersion::V1`]).
    ///
    /// Equivalent to [`DiskChunkProvider::with_history`] with
    /// `save_history = false`. The directory does **not** need to exist
    /// yet; it will be created on the first `save` call.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_history(dir, false)
    }

    /// Creates a provider with explicit control over whether the chunk's
    /// confirmed history is persisted on save.
    ///
    /// See the type-level docs for the meaning of `save_history`.
    pub fn with_history(dir: impl Into<PathBuf>, save_history: bool) -> Self {
        Self {
            dir: dir.into(),
            save_history,
        }
    }

    /// Returns whether this provider persists the confirmed history on save.
    pub fn save_history(&self) -> bool {
        self.save_history
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
    ///
    /// When `save_history` is `false` and the chunk's confirmed history is
    /// non-empty, the history is silently dropped (logged at `debug!`).
    /// This is expected behaviour, not a bug: the V1 backend explicitly
    /// trades history persistence for file size.
    pub fn save(&self, chunk: &Chunk) -> std::io::Result<()> {
        let path = self.chunk_path(chunk.position());

        let version = if self.save_history {
            ChunkVersion::V1Versioned
        } else {
            if !chunk.confirmed_history().is_empty() {
                debug!(
                    "DiskChunkProvider: dropping {}-entry confirmed history for {} (save_history = false)",
                    chunk.confirmed_history().len(),
                    chunk.position(),
                );
            }
            ChunkVersion::V1
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        serialize_chunk_versioned(chunk, std::io::BufWriter::new(file), version).map_err(
            |e| match e {
                ChunkSerializeError::Io(io_err) => io_err,
                other => Error::new(ErrorKind::Other, other.to_string()),
            },
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::deserialize_chunk;
    use dd40_core::chunk::change::ChunkChange;
    use dd40_core::prelude::*;
    use std::io::{BufReader, Read};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static UNIQ: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir(label: &str) -> PathBuf {
        let n = UNIQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "dd40_provider_test_{}_{}_{}_{}",
            label,
            std::process::id(),
            n,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_version_tag(path: &std::path::Path) -> u16 {
        let mut f = std::fs::File::open(path).unwrap();
        let mut buf = [0u8; 6];
        f.read_exact(&mut buf).unwrap();
        u16::from_le_bytes([buf[4], buf[5]])
    }

    fn make_chunk_with_history(pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        chunk.set(1, 2, 3, Block::new(BlockId(7)));
        chunk.set_version(5);
        chunk.push_confirmed_for_load(
            5,
            ChunkChange::Place {
                local: BlockLocal::new(1, 2, 3),
                block_id: BlockId(7),
            },
        );
        chunk
    }

    #[test]
    fn save_writes_v1_format_when_history_disabled() {
        let dir = tmp_dir("v1");
        let provider = DiskChunkProvider::with_history(&dir, false);
        let pos = ChunkPos::new(0, 0, 0);
        let chunk = Chunk::new(pos);
        provider.save(&chunk).unwrap();
        assert_eq!(
            read_version_tag(&dir.join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z))),
            ChunkVersion::V1.as_u16(),
        );
    }

    #[test]
    fn save_writes_v1_versioned_format_when_history_enabled() {
        let dir = tmp_dir("v1ver");
        let provider = DiskChunkProvider::with_history(&dir, true);
        let pos = ChunkPos::new(1, 0, 1);
        let chunk = Chunk::new(pos);
        provider.save(&chunk).unwrap();
        assert_eq!(
            read_version_tag(&dir.join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z))),
            ChunkVersion::V1Versioned.as_u16(),
        );
    }

    #[test]
    fn save_drops_history_under_v1() {
        let dir = tmp_dir("drop");
        let provider = DiskChunkProvider::with_history(&dir, false);
        let pos = ChunkPos::new(2, 0, 2);
        let chunk = make_chunk_with_history(pos);
        assert_eq!(chunk.confirmed_history().len(), 1);
        provider.save(&chunk).unwrap();

        let path = dir.join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z));
        let restored =
            deserialize_chunk(BufReader::new(std::fs::File::open(&path).unwrap())).unwrap();
        assert_eq!(restored.version(), 5, "version preserved under V1");
        assert!(
            restored.confirmed_history().is_empty(),
            "history dropped under V1"
        );
    }

    #[test]
    fn save_preserves_history_under_v1_versioned() {
        let dir = tmp_dir("keep");
        let provider = DiskChunkProvider::with_history(&dir, true);
        let pos = ChunkPos::new(-3, 0, 4);
        let chunk = make_chunk_with_history(pos);
        provider.save(&chunk).unwrap();

        let path = dir.join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z));
        let restored =
            deserialize_chunk(BufReader::new(std::fs::File::open(&path).unwrap())).unwrap();
        assert_eq!(restored.version(), 5);
        let restored_hist: Vec<_> = restored.confirmed_history().iter().copied().collect();
        let original_hist: Vec<_> = chunk.confirmed_history().iter().copied().collect();
        assert_eq!(restored_hist, original_hist);
    }

    #[test]
    fn new_defaults_to_no_history() {
        let provider = DiskChunkProvider::new("/tmp/unused");
        assert!(!provider.save_history());
    }
}
