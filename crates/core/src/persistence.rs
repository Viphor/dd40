//! Cross-crate vocabulary for persisting Bevy entities to disk.
//!
//! [`EntityPersister`] is the extension point for any system that
//! wants its entities to survive across server restarts (loose items
//! today, NPCs/mobs/projectiles tomorrow).  Implementations live in
//! the crate that owns the entity type; the actual on-disk format and
//! save/load timing are owned by `dd40_chunk_storage`.
//!
//! # Why a trait registry
//!
//! Persistence is a domain-spanning concern: the world doesn't know
//! what a "loose item" or an "NPC" is, but every persistence sink
//! needs to be able to write them.  Hard-coding the list of
//! persistable entity types inside `dd40_chunk_storage` would force
//! every future entity to be added there — exactly the rigid coupling
//! dd40 tries to avoid.
//!
//! Instead, each entity-owning crate implements [`EntityPersister`]
//! and registers it with [`EntityPersisterRegistry`] at plugin build
//! time.  `dd40_chunk_storage` iterates the registry and calls back
//! into each persister without knowing what they store.
//!
//! # Why `&mut World`
//!
//! Persisters need full freedom to query, mutate, and spawn — they
//! frequently touch multiple components, resources, and the
//! [`Commands`] queue at once.  Bevy systems are too narrow for this;
//! exclusive world access is the simplest fit and runs naturally from
//! a one-shot save-on-exit observer or a per-chunk load system.

use std::sync::Arc;

use bevy::prelude::*;

use crate::chunk::ChunkPos;

/// A single entity persisted into a sidecar file, identified by the
/// [`EntityPersister::kind`] string that produced it.
///
/// `payload` is opaque to `dd40_chunk_storage` — only the producing
/// persister knows how to decode it.  Putting the kind tag in the
/// file (rather than relying on registration order) means files
/// survive code reorganisation and missing persisters at load time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PersistedEntity {
    /// Stable identifier matching [`EntityPersister::kind`].
    pub kind: String,
    /// Persister-defined serialised payload (typically bincode).
    pub payload: Vec<u8>,
}

/// Implemented by any crate that wants its entities to be persisted to
/// disk and restored on load.
///
/// The crate that defines the entity type implements this trait and
/// registers it via [`EntityPersisterRegistry::register`] from its
/// plugin's `build` method.
pub trait EntityPersister: Send + Sync + 'static {
    /// Stable identifier written into every persisted payload from
    /// this persister.  Must be globally unique across all registered
    /// persisters and stable across versions; renaming the type does
    /// not require renaming the kind.
    ///
    /// Convention: `crate_name.entity_name`, e.g. `loose_item_core.loose_item`.
    fn kind(&self) -> &'static str;

    /// Walks `world` for every entity this persister owns and returns
    /// `(owning_chunk, payload_bytes)` pairs.  The owning chunk is
    /// always the chunk whose volume contains the entity's centre
    /// point — see [`ChunkPos::from`] for a `Vec3`.
    ///
    /// Called once at save time (typically on `AppExit`).  The
    /// persister is free to perform any queries or look up any
    /// resources it needs.
    fn collect(&self, world: &mut World) -> Vec<(ChunkPos, Vec<u8>)>;

    /// Spawns one entity from a previously-collected payload.
    ///
    /// Called once per [`PersistedEntity`] in a sidecar whose `kind`
    /// matches [`kind`](Self::kind).  Implementations are expected to
    /// insert the full set of components the entity needs — the
    /// chunk-storage crate does not add anything beyond what the
    /// persister inserts itself.
    fn spawn(&self, world: &mut World, bytes: &[u8]);
}

/// Resource holding the list of registered [`EntityPersister`]s.
///
/// Initialised by [`crate::plugin::CorePlugin`] so persisters can be
/// registered without depending on a specific persistence backend
/// being present (a headless test app that omits
/// `dd40_chunk_storage` still accepts persister registrations — they
/// just never fire).
#[derive(Resource, Default, Clone)]
pub struct EntityPersisterRegistry {
    persisters: Vec<Arc<dyn EntityPersister>>,
}

impl EntityPersisterRegistry {
    /// Registers a persister.  Persisters are tried in registration
    /// order on load; on save, every persister contributes to every
    /// sidecar it has entities for.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if two persisters register the same
    /// [`kind`](EntityPersister::kind).  In release builds the second
    /// registration silently shadows the first; prefer to keep kinds
    /// unique.
    pub fn register<P: EntityPersister>(&mut self, persister: P) {
        let kind = persister.kind();
        debug_assert!(
            !self.persisters.iter().any(|p| p.kind() == kind),
            "duplicate EntityPersister kind: {kind}"
        );
        self.persisters.push(Arc::new(persister));
    }

    /// Returns every registered persister, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn EntityPersister>> {
        self.persisters.iter()
    }

    /// Looks up a persister by its [`kind`](EntityPersister::kind).
    pub fn by_kind(&self, kind: &str) -> Option<&Arc<dyn EntityPersister>> {
        self.persisters.iter().find(|p| p.kind() == kind)
    }

    /// Number of registered persisters.  Mostly useful in tests.
    pub fn len(&self) -> usize {
        self.persisters.len()
    }

    /// Whether any persisters are registered.
    pub fn is_empty(&self) -> bool {
        self.persisters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakePersister(&'static str);

    impl EntityPersister for FakePersister {
        fn kind(&self) -> &'static str {
            self.0
        }
        fn collect(&self, _world: &mut World) -> Vec<(ChunkPos, Vec<u8>)> {
            Vec::new()
        }
        fn spawn(&self, _world: &mut World, _bytes: &[u8]) {}
    }

    #[test]
    fn register_then_lookup_by_kind() {
        let mut reg = EntityPersisterRegistry::default();
        reg.register(FakePersister("a.thing"));
        reg.register(FakePersister("b.thing"));
        assert_eq!(reg.len(), 2);
        assert!(reg.by_kind("a.thing").is_some());
        assert!(reg.by_kind("missing").is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate EntityPersister kind")]
    fn duplicate_kind_panics_in_debug() {
        let mut reg = EntityPersisterRegistry::default();
        reg.register(FakePersister("dup"));
        reg.register(FakePersister("dup"));
    }
}
