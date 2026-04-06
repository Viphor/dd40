//! Per-chunk render state tracking.
//!
//! [`ChunkRenderState`] is a Bevy [`Resource`] that acts as the authoritative
//! map from [`ChunkPos`] to the data the renderer needs to manage a chunk's
//! mesh entity:
//!
//! - The [`Entity`] of the spawned mesh (if any).
//! - The [`LodLevel`] the mesh was built at, so the renderer can detect when
//!   the LOD has changed and a rebuild is required.
//! - A *dirty* flag that is set whenever new chunk data arrives or the LOD
//!   changes, and cleared once the mesh is rebuilt.
//!
//! # Lifecycle
//!
//! ```text
//! ChunkReady message arrives
//!         │
//!         ▼
//! mark_dirty(pos)              ← systems::mark_dirty_on_chunk_ready
//!         │
//!         ▼  (next Update frame)
//! rebuild_dirty_chunks         ← systems::rebuild_dirty_chunks
//!   ├─ despawn old mesh entity (if any)
//!   ├─ build new Mesh from chunk data
//!   └─ spawn new mesh entity, record in ChunkRenderState
//!         │
//!         ▼
//! LOD distance changes
//!         │
//!         ▼
//! mark_dirty_on_lod_change     ← systems::update_lod_levels
//!         │
//!         └─► rebuild_dirty_chunks (same as above)
//! ```

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use dd40_core::chunk::ChunkPos;

use crate::lod::LodLevel;

// ── Per-chunk entry ───────────────────────────────────────────────────────────

/// Render metadata for a single chunk.
///
/// Stored inside [`ChunkRenderState`] keyed by [`ChunkPos`].
#[derive(Debug, Clone)]
pub struct ChunkEntry {
    /// The ECS entity that owns the `Mesh3d` / `MeshMaterial3d` components for
    /// this chunk, or `None` if no mesh has been spawned yet (e.g. the chunk
    /// is all-air or has never been meshed).
    pub mesh_entity: Option<Entity>,
    /// The [`LodLevel`] at which the current mesh was built.  Used to detect
    /// when the player has moved far enough to trigger a LOD change.
    pub current_lod: LodLevel,
    /// `true` when the chunk's mesh needs to be (re-)built on the next
    /// [`Update`] tick.  Set by [`ChunkRenderState::mark_dirty`] and cleared
    /// by [`ChunkRenderState::clear_dirty`].
    pub dirty: bool,
}

impl ChunkEntry {
    /// Creates a new entry with no mesh entity, [`LodLevel::Lod0`], and
    /// `dirty = true` so the chunk is meshed on the very next frame.
    pub fn new_dirty() -> Self {
        Self {
            mesh_entity: None,
            current_lod: LodLevel::Lod0,
            dirty: true,
        }
    }
}

// ── ChunkRenderState ──────────────────────────────────────────────────────────

/// Bevy resource that tracks the render state for every known chunk.
///
/// This is the single source of truth for the renderer: it maps each
/// [`ChunkPos`] to the corresponding [`ChunkEntry`] and provides helpers to
/// mark chunks dirty, update their LOD level, and query which chunks need
/// rebuilding.
///
/// # Usage
///
/// Systems that produce new chunk data (e.g. [`systems::mark_dirty_on_chunk_ready`])
/// call [`ChunkRenderState::mark_dirty`].  The mesh-rebuild system queries
/// [`ChunkRenderState::dirty_chunks`] each frame and calls
/// [`ChunkRenderState::clear_dirty`] after rebuilding.
#[derive(Resource, Default)]
pub struct ChunkRenderState {
    entries: HashMap<ChunkPos, ChunkEntry>,
}

impl ChunkRenderState {
    /// Creates a new, empty render state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the chunk at `pos` as dirty so it will be re-meshed next frame.
    ///
    /// If no entry exists for `pos` yet, a new one is created.
    pub fn mark_dirty(&mut self, pos: ChunkPos) {
        self.entries
            .entry(pos)
            .or_insert_with(ChunkEntry::new_dirty)
            .dirty = true;
    }

    /// Clears the dirty flag for the chunk at `pos`.
    ///
    /// Has no effect if `pos` has no entry.
    pub fn clear_dirty(&mut self, pos: ChunkPos) {
        if let Some(entry) = self.entries.get_mut(&pos) {
            entry.dirty = false;
        }
    }

    /// Returns the [`Entity`] of the mesh spawned for `pos`, if any.
    pub fn mesh_entity(&self, pos: &ChunkPos) -> Option<Entity> {
        self.entries.get(pos)?.mesh_entity
    }

    /// Stores (or replaces) the mesh entity for `pos`.
    ///
    /// If no entry exists yet, one is created with `dirty = false` and
    /// [`LodLevel::Lod0`].
    pub fn set_mesh_entity(&mut self, pos: ChunkPos, entity: Option<Entity>) {
        let entry = self
            .entries
            .entry(pos)
            .or_insert_with(ChunkEntry::new_dirty);
        entry.mesh_entity = entity;
    }

    /// Returns the [`LodLevel`] at which the chunk at `pos` was last meshed.
    ///
    /// Returns [`LodLevel::Lod0`] when no entry exists yet.
    pub fn current_lod(&self, pos: &ChunkPos) -> LodLevel {
        self.entries
            .get(pos)
            .map(|e| e.current_lod)
            .unwrap_or(LodLevel::Lod0)
    }

    /// Updates the recorded [`LodLevel`] for `pos` and marks it dirty if the
    /// level has changed.
    ///
    /// Returns `true` when the level changed and the entry was marked dirty.
    pub fn update_lod(&mut self, pos: ChunkPos, new_lod: LodLevel) -> bool {
        let entry = self
            .entries
            .entry(pos)
            .or_insert_with(ChunkEntry::new_dirty);

        if entry.current_lod != new_lod {
            entry.current_lod = new_lod;
            entry.dirty = true;
            true
        } else {
            false
        }
    }

    /// Returns an iterator over all chunk positions that are currently dirty.
    ///
    /// The iterator yields `ChunkPos` values in arbitrary order.
    pub fn dirty_chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.entries
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(&pos, _)| pos)
    }

    /// Returns a shared reference to the [`ChunkEntry`] for `pos`, or `None`.
    pub fn get(&self, pos: &ChunkPos) -> Option<&ChunkEntry> {
        self.entries.get(pos)
    }

    /// Returns a mutable reference to the [`ChunkEntry`] for `pos`, or `None`.
    pub fn get_mut(&mut self, pos: &ChunkPos) -> Option<&mut ChunkEntry> {
        self.entries.get_mut(pos)
    }

    /// Removes the entry for `pos` entirely and returns it, if it existed.
    ///
    /// The caller is responsible for despawning any associated mesh entity
    /// before calling this.
    pub fn remove(&mut self, pos: &ChunkPos) -> Option<ChunkEntry> {
        self.entries.remove(pos)
    }

    /// Returns the total number of tracked chunks (dirty or not).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when no chunks are being tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::chunk::ChunkPos;

    fn pos(x: i32, z: i32) -> ChunkPos {
        ChunkPos::new(x, z)
    }

    // ── Initial state ─────────────────────────────────────────────────────────

    #[test]
    fn new_state_is_empty() {
        let state = ChunkRenderState::new();
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);
    }

    // ── mark_dirty / clear_dirty ──────────────────────────────────────────────

    #[test]
    fn mark_dirty_creates_entry() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(1, 2));
        assert_eq!(state.len(), 1);
        assert!(state.get(&pos(1, 2)).unwrap().dirty);
    }

    #[test]
    fn clear_dirty_clears_flag() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        state.clear_dirty(pos(0, 0));
        assert!(!state.get(&pos(0, 0)).unwrap().dirty);
    }

    #[test]
    fn clear_dirty_on_missing_pos_is_noop() {
        let mut state = ChunkRenderState::new();
        // Should not panic.
        state.clear_dirty(pos(99, 99));
        assert!(state.is_empty());
    }

    #[test]
    fn mark_dirty_twice_stays_dirty() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(3, 3));
        state.clear_dirty(pos(3, 3));
        state.mark_dirty(pos(3, 3));
        assert!(state.get(&pos(3, 3)).unwrap().dirty);
    }

    // ── dirty_chunks iterator ─────────────────────────────────────────────────

    #[test]
    fn dirty_chunks_yields_only_dirty_entries() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        state.mark_dirty(pos(1, 0));
        state.mark_dirty(pos(2, 0));
        state.clear_dirty(pos(1, 0)); // pos(1,0) is now clean

        let dirty: Vec<ChunkPos> = state.dirty_chunks().collect();
        assert_eq!(dirty.len(), 2);
        assert!(dirty.contains(&pos(0, 0)));
        assert!(!dirty.contains(&pos(1, 0)));
        assert!(dirty.contains(&pos(2, 0)));
    }

    #[test]
    fn dirty_chunks_empty_when_all_clean() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(5, 5));
        state.clear_dirty(pos(5, 5));
        assert_eq!(state.dirty_chunks().count(), 0);
    }

    // ── mesh entity ───────────────────────────────────────────────────────────

    #[test]
    fn mesh_entity_none_before_set() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        assert!(state.mesh_entity(&pos(0, 0)).is_none());
    }

    #[test]
    fn set_and_get_mesh_entity() {
        let mut state = ChunkRenderState::new();
        // We cannot create a real Entity without a World, but we can use
        // Entity::from_bits which takes a u64 encoding of index + generation.
        let fake_entity = Entity::from_bits(42);
        state.set_mesh_entity(pos(1, 1), Some(fake_entity));
        assert_eq!(state.mesh_entity(&pos(1, 1)), Some(fake_entity));
    }

    #[test]
    fn set_mesh_entity_none_clears_it() {
        let mut state = ChunkRenderState::new();
        let e = Entity::from_bits(7);
        state.set_mesh_entity(pos(0, 0), Some(e));
        state.set_mesh_entity(pos(0, 0), None);
        assert!(state.mesh_entity(&pos(0, 0)).is_none());
    }

    // ── LOD management ────────────────────────────────────────────────────────

    #[test]
    fn default_lod_is_lod0() {
        let state = ChunkRenderState::new();
        assert_eq!(state.current_lod(&pos(0, 0)), LodLevel::Lod0);
    }

    #[test]
    fn update_lod_to_same_level_does_not_mark_dirty() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        state.clear_dirty(pos(0, 0)); // start clean at Lod0

        let changed = state.update_lod(pos(0, 0), LodLevel::Lod0);
        assert!(!changed);
        assert!(!state.get(&pos(0, 0)).unwrap().dirty);
    }

    #[test]
    fn update_lod_to_different_level_marks_dirty() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        state.clear_dirty(pos(0, 0)); // start clean at Lod0

        let changed = state.update_lod(pos(0, 0), LodLevel::Lod1);
        assert!(changed);
        assert!(state.get(&pos(0, 0)).unwrap().dirty);
        assert_eq!(state.current_lod(&pos(0, 0)), LodLevel::Lod1);
    }

    #[test]
    fn update_lod_creates_entry_when_missing() {
        let mut state = ChunkRenderState::new();
        state.update_lod(pos(5, 5), LodLevel::Lod2);
        assert!(state.get(&pos(5, 5)).is_some());
        assert_eq!(state.current_lod(&pos(5, 5)), LodLevel::Lod2);
    }

    // ── remove ────────────────────────────────────────────────────────────────

    #[test]
    fn remove_deletes_entry() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(3, 3));
        let removed = state.remove(&pos(3, 3));
        assert!(removed.is_some());
        assert!(state.is_empty());
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut state = ChunkRenderState::new();
        assert!(state.remove(&pos(0, 0)).is_none());
    }

    // ── multiple chunks ───────────────────────────────────────────────────────

    #[test]
    fn multiple_chunks_tracked_independently() {
        let mut state = ChunkRenderState::new();
        state.mark_dirty(pos(0, 0));
        state.mark_dirty(pos(1, 0));
        state.mark_dirty(pos(0, 1));

        state.clear_dirty(pos(1, 0));
        state.update_lod(pos(0, 1), LodLevel::Lod2);

        assert!(state.get(&pos(0, 0)).unwrap().dirty);
        assert!(!state.get(&pos(1, 0)).unwrap().dirty);
        assert!(state.get(&pos(0, 1)).unwrap().dirty);
        assert_eq!(state.current_lod(&pos(0, 1)), LodLevel::Lod2);
        assert_eq!(state.len(), 3);
    }
}
