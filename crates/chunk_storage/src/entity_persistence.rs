//! Entity persistence systems for `DiskStoragePlugin`.
//!
//! Two systems drive the per-chunk entity sidecar file:
//!
//! - [`load_entities_for_ready_chunks`] runs after every [`ChunkReady`]
//!   message, reads the matching `entities_X_Y_Z.bin` if it exists,
//!   and dispatches each [`PersistedEntity`] to its registered
//!   [`EntityPersister`].
//! - [`save_entities_on_exit`] runs in [`Last`] whenever an [`AppExit`]
//!   message is observed.  It walks every registered persister,
//!   groups the returned payloads by owning chunk, and writes one
//!   sidecar file per chunk.
//!
//! Both systems are no-ops when [`EntityPersistenceConfig::enabled`]
//! is `false`, when no persisters are registered, or when no chunks
//! match.  Missing sidecars on load are treated as "no entities" and
//! logged at `debug!`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::app::AppExit;
use bevy::prelude::*;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::prelude::*;

use crate::entity_sidecar::{
    EntitySidecarError, deserialize_entities, entity_sidecar_path, serialize_entities,
};

/// Environment variable controlling whether the sidecar systems are active.
///
/// Truthy values (`1` / `true` / `yes` / `on`, case-insensitive) enable
/// persistence; anything else disables it.  Default when unset: `true`.
pub const SAVE_ENTITIES_ENV: &str = "DD40_CHUNK_STORAGE__SAVE_ENTITIES";

/// Configuration resource controlling the sidecar systems at runtime.
///
/// Inserted by [`crate::plugin::DiskStoragePlugin`] from the
/// [`SAVE_ENTITIES_ENV`] environment variable.  Tests and bootstrappers
/// can override the value before plugin add to force a known state.
#[derive(Resource, Debug, Clone)]
pub struct EntityPersistenceConfig {
    /// Whether load and save are both active.  Disabling skips disk
    /// I/O entirely; existing sidecars are left untouched.
    pub enabled: bool,
    /// Directory the sidecar files live in.  Inserted by
    /// [`crate::plugin::DiskStoragePlugin`] to match the chunk
    /// directory so siblings share a folder.
    pub dir: PathBuf,
}

/// Parses the value of [`SAVE_ENTITIES_ENV`] the same way
/// [`crate::plugin::SAVE_HISTORY_ENV`] is parsed.
pub fn parse_save_entities_value(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Reads [`SAVE_ENTITIES_ENV`] from the process environment.  When the
/// variable is unset, persistence defaults to **enabled** — losing
/// loose items on every server restart is more jarring than the cost
/// of writing one file per loaded chunk on exit.
pub fn read_save_entities_env() -> bool {
    match std::env::var(SAVE_ENTITIES_ENV) {
        Ok(raw) => parse_save_entities_value(&raw),
        Err(_) => true,
    }
}

/// Exclusive-world system that reads sidecars for any [`ChunkReady`]
/// messages produced this frame and dispatches their payloads through
/// the registered [`EntityPersister`]s.
///
/// Runs as an exclusive system because persisters take `&mut World`.
pub fn load_entities_for_ready_chunks(world: &mut World) {
    if !world
        .get_resource::<EntityPersistenceConfig>()
        .map(|c| c.enabled)
        .unwrap_or(false)
    {
        return;
    }
    if world
        .get_resource::<EntityPersisterRegistry>()
        .map(|r| r.is_empty())
        .unwrap_or(true)
    {
        return;
    }

    let ready_positions: Vec<ChunkPos> = world
        .resource::<Messages<ChunkReady>>()
        .iter_current_update_messages()
        .map(|m| m.chunk.position())
        .collect();
    if ready_positions.is_empty() {
        return;
    }

    let dir = world.resource::<EntityPersistenceConfig>().dir.clone();
    let registry = world.resource::<EntityPersisterRegistry>().clone();

    for pos in ready_positions {
        let path = entity_sidecar_path(&dir, pos);
        let entities = match read_sidecar(&path, pos) {
            Ok(es) => es,
            Err(SidecarReadOutcome::NotFound) => {
                debug!("no entity sidecar for {pos} at {path:?}");
                continue;
            }
            Err(SidecarReadOutcome::Error(err)) => {
                warn!("failed to read entity sidecar at {path:?}: {err}");
                continue;
            }
        };

        for entity in entities {
            match registry.by_kind(&entity.kind) {
                Some(persister) => persister.clone().spawn(world, &entity.payload),
                None => warn!(
                    "no EntityPersister registered for kind {:?} (chunk {pos}); skipping payload",
                    entity.kind
                ),
            }
        }
    }
}

/// Exclusive-world system that runs in [`Last`] each frame and, when
/// an [`AppExit`] message is present, persists every entity owned by
/// every registered persister to its sidecar file.
///
/// Only the latest frame's `AppExit` triggers a write; subsequent
/// frames (which can happen if the runner takes more than one tick to
/// actually shut down) are skipped via `AlreadySavedEntities`.
pub fn save_entities_on_exit(world: &mut World) {
    if world.contains_resource::<AlreadySavedEntities>() {
        return;
    }
    if world
        .resource::<Messages<AppExit>>()
        .iter_current_update_messages()
        .next()
        .is_none()
    {
        return;
    }
    if !world
        .get_resource::<EntityPersistenceConfig>()
        .map(|c| c.enabled)
        .unwrap_or(false)
    {
        world.insert_resource(AlreadySavedEntities);
        return;
    }

    info!("AppExit observed — flushing entity sidecars to disk");
    save_all_entities(world);
    world.insert_resource(AlreadySavedEntities);
}

/// Marker inserted after [`save_entities_on_exit`] has run once so
/// repeat AppExit frames do not re-flush.
#[derive(Resource)]
struct AlreadySavedEntities;

/// Public API: walks every registered persister, groups the returned
/// payloads by chunk, and writes one sidecar per chunk.
///
/// Any chunk currently in [`ChunkCache`] that ends up with **no**
/// entities has its sidecar file removed (if one exists), so a chunk
/// whose entities have all been picked up / despawned does not
/// re-spawn its stale contents on the next load.
///
/// Useful for tests and for any future per-chunk eviction system that
/// wants to flush a single chunk on demand (which can be added by
/// composing this with a `for_chunk: Option<ChunkPos>` filter).
pub fn save_all_entities(world: &mut World) {
    let registry = world.resource::<EntityPersisterRegistry>().clone();
    if registry.is_empty() {
        return;
    }
    let dir = world.resource::<EntityPersistenceConfig>().dir.clone();

    let mut by_chunk: HashMap<ChunkPos, Vec<PersistedEntity>> = HashMap::new();
    for persister in registry.iter() {
        for (chunk, payload) in persister.collect(world) {
            by_chunk
                .entry(chunk)
                .or_default()
                .push(PersistedEntity {
                    kind: persister.kind().to_string(),
                    payload,
                });
        }
    }

    // Collect the set of currently-loaded chunk positions so we can
    // remove stale sidecars: a chunk that previously had entities but
    // is now empty must not keep its old sidecar on disk, otherwise
    // the entities reappear (duplicated) on the next load.
    let loaded_positions: HashSet<ChunkPos> = world
        .get_resource::<ChunkCache>()
        .map(|cache| cache.iter_positions().copied().collect())
        .unwrap_or_default();

    let stale_positions: Vec<ChunkPos> = loaded_positions
        .into_iter()
        .filter(|pos| !by_chunk.contains_key(pos))
        .collect();

    if by_chunk.is_empty() && stale_positions.is_empty() {
        debug!("entity-sidecar flush: nothing to write");
        return;
    }

    if !by_chunk.is_empty() {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            warn!("failed to create entity-sidecar directory {dir:?}: {err}");
            return;
        }
    }

    for (chunk, entities) in by_chunk {
        let path = entity_sidecar_path(&dir, chunk);
        if let Err(err) = write_sidecar(&path, chunk, &entities) {
            warn!("failed to write entity sidecar {path:?}: {err}");
        } else {
            debug!(
                "wrote {} entity payload(s) to {path:?}",
                entities.len()
            );
        }
    }

    for chunk in stale_positions {
        let path = entity_sidecar_path(&dir, chunk);
        match std::fs::remove_file(&path) {
            Ok(()) => debug!("removed stale entity sidecar {path:?} (chunk {chunk} now empty)"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => warn!("failed to remove stale entity sidecar {path:?}: {err}"),
        }
    }
}

enum SidecarReadOutcome {
    NotFound,
    Error(EntitySidecarError),
}

fn read_sidecar(
    path: &Path,
    expected: ChunkPos,
) -> Result<Vec<PersistedEntity>, SidecarReadOutcome> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SidecarReadOutcome::NotFound);
        }
        Err(e) => return Err(SidecarReadOutcome::Error(EntitySidecarError::Io(e))),
    };
    deserialize_entities(std::io::BufReader::new(file), expected)
        .map_err(SidecarReadOutcome::Error)
}

fn write_sidecar(
    path: &Path,
    pos: ChunkPos,
    entities: &[PersistedEntity],
) -> Result<(), EntitySidecarError> {
    let file = std::fs::File::create(path)?;
    serialize_entities(std::io::BufWriter::new(file), pos, entities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parse_truthy_values() {
        for v in ["1", "true", "TRUE", "yes", "On", " on "] {
            assert!(parse_save_entities_value(v), "expected truthy: {v:?}");
        }
    }

    #[test]
    fn parse_falsy_values() {
        for v in ["0", "false", "no", "off", ""] {
            assert!(!parse_save_entities_value(v), "expected falsy: {v:?}");
        }
    }

    static UNIQ: AtomicU32 = AtomicU32::new(0);

    fn tmp_dir(label: &str) -> PathBuf {
        let n = UNIQ.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("dd40_ent_{label}_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A persister that records spawn calls and produces a single fake
    /// payload per chunk in `to_emit` on collect.
    struct RecordingPersister {
        kind: &'static str,
        spawned: Arc<Mutex<Vec<Vec<u8>>>>,
        to_emit: Vec<(ChunkPos, Vec<u8>)>,
    }

    impl EntityPersister for RecordingPersister {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn collect(&self, _world: &mut World) -> Vec<(ChunkPos, Vec<u8>)> {
            self.to_emit.clone()
        }
        fn spawn(&self, _world: &mut World, bytes: &[u8]) {
            self.spawned.lock().unwrap().push(bytes.to_vec());
        }
    }

    fn make_app(dir: PathBuf) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.add_message::<AppExit>();
        app.add_message::<ChunkReady>();
        app.init_resource::<EntityPersisterRegistry>();
        app.init_resource::<dd40_core::chunk::cache::ChunkCache>();
        app.insert_resource(EntityPersistenceConfig { enabled: true, dir });
        app
    }

    fn make_chunk(pos: ChunkPos) -> Chunk {
        Chunk::new(pos)
    }

    #[test]
    fn load_dispatches_payloads_to_matching_persister() {
        let dir = tmp_dir("load_dispatch");
        let pos = ChunkPos::new(2, 0, -3);
        // Pre-write a sidecar.
        let entities = vec![PersistedEntity {
            kind: "test.thing".into(),
            payload: vec![9, 8, 7],
        }];
        let file = std::fs::File::create(entity_sidecar_path(&dir, pos)).unwrap();
        serialize_entities(std::io::BufWriter::new(file), pos, &entities).unwrap();

        let mut app = make_app(dir);
        let spawned = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: spawned.clone(),
                to_emit: Vec::new(),
            });

        app.world_mut()
            .resource_mut::<Messages<ChunkReady>>()
            .write(ChunkReady { chunk: make_chunk(pos) });
        load_entities_for_ready_chunks(app.world_mut());

        let got = spawned.lock().unwrap().clone();
        assert_eq!(got, vec![vec![9, 8, 7]]);
    }

    #[test]
    fn load_missing_sidecar_is_a_silent_noop() {
        let dir = tmp_dir("load_missing");
        let pos = ChunkPos::new(0, 0, 0);

        let mut app = make_app(dir);
        let spawned = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: spawned.clone(),
                to_emit: Vec::new(),
            });
        app.world_mut()
            .resource_mut::<Messages<ChunkReady>>()
            .write(ChunkReady { chunk: make_chunk(pos) });
        load_entities_for_ready_chunks(app.world_mut());

        assert!(spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn load_is_disabled_when_config_disabled() {
        let dir = tmp_dir("load_disabled");
        let pos = ChunkPos::new(1, 0, 1);
        let entities = vec![PersistedEntity {
            kind: "test.thing".into(),
            payload: vec![1],
        }];
        let file = std::fs::File::create(entity_sidecar_path(&dir, pos)).unwrap();
        serialize_entities(std::io::BufWriter::new(file), pos, &entities).unwrap();

        let mut app = make_app(dir);
        app.world_mut().resource_mut::<EntityPersistenceConfig>().enabled = false;
        let spawned = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: spawned.clone(),
                to_emit: Vec::new(),
            });
        app.world_mut()
            .resource_mut::<Messages<ChunkReady>>()
            .write(ChunkReady { chunk: make_chunk(pos) });
        load_entities_for_ready_chunks(app.world_mut());

        assert!(spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn save_all_writes_one_sidecar_per_chunk() {
        let dir = tmp_dir("save_grouped");
        let mut app = make_app(dir.clone());
        let spawned = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: spawned.clone(),
                to_emit: vec![
                    (ChunkPos::new(0, 0, 0), vec![1]),
                    (ChunkPos::new(0, 0, 0), vec![2]),
                    (ChunkPos::new(1, 0, 0), vec![3]),
                ],
            });

        save_all_entities(app.world_mut());

        // Verify both files exist and round-trip.
        let a = entity_sidecar_path(&dir, ChunkPos::new(0, 0, 0));
        let b = entity_sidecar_path(&dir, ChunkPos::new(1, 0, 0));
        let read_a = deserialize_entities(
            std::io::BufReader::new(std::fs::File::open(&a).unwrap()),
            ChunkPos::new(0, 0, 0),
        )
        .unwrap();
        let read_b = deserialize_entities(
            std::io::BufReader::new(std::fs::File::open(&b).unwrap()),
            ChunkPos::new(1, 0, 0),
        )
        .unwrap();
        let payloads_a: Vec<_> = read_a.into_iter().map(|e| e.payload).collect();
        let payloads_b: Vec<_> = read_b.into_iter().map(|e| e.payload).collect();
        assert_eq!(payloads_a, vec![vec![1], vec![2]]);
        assert_eq!(payloads_b, vec![vec![3]]);
    }

    #[test]
    fn save_on_exit_only_fires_when_app_exit_present() {
        let dir = tmp_dir("save_on_exit");
        let mut app = make_app(dir.clone());
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: Arc::new(Mutex::new(Vec::new())),
                to_emit: vec![(ChunkPos::new(7, 0, 0), vec![42])],
            });

        // No AppExit yet → nothing written.
        save_entities_on_exit(app.world_mut());
        assert!(
            !entity_sidecar_path(&dir, ChunkPos::new(7, 0, 0)).exists(),
            "sidecar should not exist before AppExit"
        );

        // Fire AppExit and re-run.
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .write(AppExit::Success);
        save_entities_on_exit(app.world_mut());
        assert!(
            entity_sidecar_path(&dir, ChunkPos::new(7, 0, 0)).exists(),
            "sidecar should exist after AppExit"
        );
    }

    #[test]
    fn save_removes_stale_sidecar_for_loaded_chunk_that_is_now_empty() {
        let dir = tmp_dir("stale_cleanup");
        let pos = ChunkPos::new(2, 0, 3);

        // 1) First save: chunk has one entity → sidecar is written.
        let mut app = make_app(dir.clone());
        app.world_mut()
            .resource_mut::<dd40_core::chunk::cache::ChunkCache>()
            .insert(make_chunk(pos));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: Arc::new(Mutex::new(Vec::new())),
                to_emit: vec![(pos, vec![1, 2, 3])],
            });
        save_all_entities(app.world_mut());
        let path = entity_sidecar_path(&dir, pos);
        assert!(path.exists(), "sidecar should be written on first save");

        // 2) Second save: same chunk is still loaded but the persister
        //    now reports zero payloads. The stale sidecar must be deleted.
        let mut app = make_app(dir.clone());
        app.world_mut()
            .resource_mut::<dd40_core::chunk::cache::ChunkCache>()
            .insert(make_chunk(pos));
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: Arc::new(Mutex::new(Vec::new())),
                to_emit: Vec::new(),
            });
        save_all_entities(app.world_mut());
        assert!(
            !path.exists(),
            "stale sidecar should be removed when chunk has no entities on save"
        );
    }

    #[test]
    fn save_does_not_touch_sidecars_for_unloaded_chunks() {
        let dir = tmp_dir("unloaded_untouched");
        let pos = ChunkPos::new(9, 0, 9);

        // Pre-write a sidecar for a chunk that is NOT loaded.
        let entities = vec![PersistedEntity {
            kind: "test.thing".into(),
            payload: vec![7],
        }];
        let file = std::fs::File::create(entity_sidecar_path(&dir, pos)).unwrap();
        serialize_entities(std::io::BufWriter::new(file), pos, &entities).unwrap();

        // Run a save where no chunks are loaded and persister emits nothing.
        let mut app = make_app(dir.clone());
        app.world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: Arc::new(Mutex::new(Vec::new())),
                to_emit: Vec::new(),
            });
        save_all_entities(app.world_mut());

        assert!(
            entity_sidecar_path(&dir, pos).exists(),
            "sidecar for unloaded chunk must not be removed"
        );
    }

    #[test]
    fn save_then_load_roundtrips_through_disk() {
        let dir = tmp_dir("roundtrip");
        let pos = ChunkPos::new(4, 0, -4);

        // 1) Save.
        let mut save_app = make_app(dir.clone());
        save_app
            .world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: Arc::new(Mutex::new(Vec::new())),
                to_emit: vec![(pos, vec![11, 22, 33])],
            });
        save_all_entities(save_app.world_mut());

        // 2) Load in a fresh app.
        let mut load_app = make_app(dir);
        let spawned = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        load_app
            .world_mut()
            .resource_mut::<EntityPersisterRegistry>()
            .register(RecordingPersister {
                kind: "test.thing",
                spawned: spawned.clone(),
                to_emit: Vec::new(),
            });
        load_app
            .world_mut()
            .resource_mut::<Messages<ChunkReady>>()
            .write(ChunkReady { chunk: make_chunk(pos) });
        load_entities_for_ready_chunks(load_app.world_mut());

        assert_eq!(spawned.lock().unwrap().clone(), vec![vec![11, 22, 33]]);
    }
}
