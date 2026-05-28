//! Save every cached chunk to disk on [`AppExit`].
//!
//! Without this, [`DiskChunkProvider::save`] is only ever called from
//! tests and a freshly generated chunk that was never persisted before
//! the server stops is regenerated from scratch on the next run —
//! silently losing any committed mutations to it.
//!
//! Runs in `Last` so it observes the same `AppExit` window that
//! [`crate::entity_persistence::save_entities_on_exit`] does.  Both
//! systems are idempotent (a `Local<bool>` gate) so subsequent frames
//! that still see `AppExit` are no-ops.

use bevy::app::AppExit;
use bevy::ecs::message::MessageReader;
use bevy::log::{error, info};
use bevy::prelude::*;
use dd40_core::chunk::cache::ChunkCache;

use crate::provider::DiskChunkProvider;

/// `Last`-schedule system: on the first frame that observes [`AppExit`],
/// writes every chunk currently in [`ChunkCache`] through the
/// configured [`DiskChunkProvider`].
pub fn save_chunks_on_exit(
    mut exits: MessageReader<AppExit>,
    mut done: Local<bool>,
    cache: Res<ChunkCache>,
    provider: Res<DiskChunkProvider>,
) {
    if *done {
        return;
    }
    if exits.read().next().is_none() {
        return;
    }
    *done = true;

    let positions: Vec<_> = cache.iter_positions().copied().collect();
    let mut saved = 0usize;
    let mut errors = 0usize;
    for pos in positions {
        let Some(chunk) = cache.get(&pos) else {
            continue;
        };
        match provider.save(chunk) {
            Ok(()) => saved += 1,
            Err(e) => {
                error!("save_chunks_on_exit: failed to save chunk {pos:?}: {e}");
                errors += 1;
            }
        }
    }
    info!("save_chunks_on_exit: persisted {saved} chunk(s) on AppExit ({errors} error(s))");
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockDataTypeRegistry;
    use dd40_core::chunk::{Chunk, ChunkPos};
    use std::sync::atomic::{AtomicU32, Ordering};

    static UNIQ: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir() -> std::path::PathBuf {
        let n = UNIQ.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("dd40_chunks_save_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn chunk_path(dir: &std::path::Path, pos: ChunkPos) -> std::path::PathBuf {
        dir.join(format!("chunk_{}_{}_{}.bin", pos.x, pos.y, pos.z))
    }

    fn make_app(dir: std::path::PathBuf) -> App {
        let mut app = App::new();
        app.add_message::<AppExit>();
        let mut provider = DiskChunkProvider::new(dir);
        provider.set_registry(BlockDataTypeRegistry::default());
        app.insert_resource(provider);
        app.init_resource::<ChunkCache>();
        app.add_systems(Last, save_chunks_on_exit);
        app
    }

    #[test]
    fn no_app_exit_means_no_writes() {
        let dir = tmp_dir();
        let mut app = make_app(dir.clone());
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .insert(Chunk::new(ChunkPos::new(0, 0, 0)));
        for _ in 0..3 {
            app.update();
        }
        assert!(
            !chunk_path(&dir, ChunkPos::new(0, 0, 0)).exists(),
            "no chunk file should exist when AppExit was never fired"
        );
    }

    #[test]
    fn app_exit_persists_every_cached_chunk_once() {
        let dir = tmp_dir();
        let mut app = make_app(dir.clone());
        let positions = [
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(1, 0, 2),
            ChunkPos::new(-3, 0, 4),
        ];
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            for p in &positions {
                cache.insert(Chunk::new(*p));
            }
        }
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .write(AppExit::Success);

        for _ in 0..5 {
            app.update();
        }

        for p in &positions {
            assert!(
                chunk_path(&dir, *p).exists(),
                "chunk {p:?} should have been written on AppExit"
            );
        }
    }
}
