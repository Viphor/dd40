//! Chunk-keyed spatial cache for [`CharacterCollider`] entities.
//!
//! # Problem
//!
//! A naïve O(n²) pair-scan over *all* character colliders in the world is
//! wasteful: two characters on opposite sides of the map can never collide.
//! We can cull the candidate set down to only characters that share at least
//! one chunk cell.
//!
//! # Design
//!
//! [`CharacterSpatialCache`] is a [`Resource`] that maps each [`ChunkPos`] to
//! the set of [`Entity`] handles whose AABB currently overlaps that chunk's
//! XZ footprint.
//!
//! A character whose AABB straddles a chunk boundary will appear in **all**
//! overlapping chunks simultaneously, so cross-boundary collisions are never
//! missed.
//!
//! ## Update cadence
//!
//! [`update_character_spatial_cache`] runs at the start of
//! [`PhysicsSet::CharacterCollision`] (before the pair-scan) and rebuilds the
//! cache from the current [`TentativePosition`]s.  It runs every fixed tick so
//! positions are always fresh.
//!
//! ## Pair deduplication
//!
//! Two characters that straddle the same boundary will appear together in
//! multiple chunks.  [`CharacterSpatialCache::candidate_pairs`] returns each
//! `(Entity, Entity)` pair **at most once** using a [`HashSet`] keyed on
//! `(min(a, b), max(a, b))`.
//!
//! # Complexity
//!
//! - Cache rebuild: **O(n)** in the number of characters (each character
//!   touches at most 4 chunks at once on a 2-D grid).
//! - Pair scan: **O(k²)** per chunk where *k* is the number of characters
//!   in that chunk — typically very small.  The global worst case is still
//!   O(n²) when all characters are in the same chunk, but the average case
//!   is far better for spread-out populations.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::prelude::*;
use dd40_core::block::BlockPos;
use dd40_core::chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Z, ChunkPos};

// ---------------------------------------------------------------------------
// Resource
// ---------------------------------------------------------------------------

/// Chunk-keyed index of all [`CharacterCollider`] entities.
///
/// Updated every [`PhysicsSet::CharacterCollision`] tick from the current
/// [`TentativePosition`]s before the pair-scan runs.
///
/// # Multi-chunk membership
///
/// A character whose AABB overlaps more than one chunk will be listed in each
/// of those chunks.  This guarantees that cross-boundary collisions are always
/// detected, at the cost of a character appearing in up to four entries.
#[derive(Resource, Default)]
pub struct CharacterSpatialCache {
    /// Map from chunk position to the entities whose AABBs overlap that chunk.
    chunks: HashMap<ChunkPos, Vec<Entity>>,
}

impl CharacterSpatialCache {
    /// Clears all entries and rebuilds the cache from the provided iterator of
    /// `(entity, world-space foot-origin, aabb)` tuples.
    ///
    /// This is called once per fixed tick by [`update_character_spatial_cache`].
    pub fn rebuild<'a>(&mut self, entries: impl Iterator<Item = (Entity, Vec3, &'a Aabb)>) {
        self.chunks.clear();

        for (entity, origin, aabb) in entries {
            for chunk_pos in chunks_for_aabb(origin, aabb) {
                self.chunks.entry(chunk_pos).or_default().push(entity);
            }
        }
    }

    /// Returns an iterator over every unique `(Entity, Entity)` pair that
    /// share at least one chunk, i.e. **candidate collision pairs**.
    ///
    /// Each pair `(a, b)` is emitted exactly once regardless of how many
    /// chunks the two entities share.  The smaller [`Entity`] id is always
    /// the first element so the deduplication key is stable.
    pub fn candidate_pairs(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        let mut seen: HashSet<(Entity, Entity)> = HashSet::new();
        let mut pairs: Vec<(Entity, Entity)> = Vec::new();

        for entities in self.chunks.values() {
            for i in 0..entities.len() {
                for j in (i + 1)..entities.len() {
                    let a = entities[i];
                    let b = entities[j];
                    let key = if a < b { (a, b) } else { (b, a) };
                    if seen.insert(key) {
                        pairs.push(key);
                    }
                }
            }
        }

        pairs.into_iter()
    }

    /// Returns a slice of all entities currently registered in `chunk_pos`,
    /// or an empty slice when the chunk has no characters.
    ///
    /// Useful for debugging and tests.
    pub fn entities_in_chunk(&self, chunk_pos: ChunkPos) -> &[Entity] {
        self.chunks
            .get(&chunk_pos)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Returns the total number of (chunk, entity) registrations.
    ///
    /// A single entity overlapping *k* chunks contributes *k* to this count.
    /// Useful for debugging memory usage.
    pub fn registration_count(&self) -> usize {
        self.chunks.values().map(Vec::len).sum()
    }

    /// Returns the number of chunks that have at least one character.
    pub fn occupied_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Returns an iterator over all entities that may overlap the 1×1×1 block
    /// cell at `block_pos`.
    ///
    /// This is a **conservative spatial filter**: it returns every entity
    /// registered in any chunk whose XZ footprint intersects the block cell.
    /// The block cell can straddle a chunk boundary, so up to four chunks may
    /// be queried.
    ///
    /// Each entity is yielded **at most once** even when it appears in multiple
    /// matching chunks (e.g. a character standing on a chunk-boundary edge).
    ///
    /// Callers must still perform a precise AABB overlap test — this method
    /// only narrows the candidate set.
    pub fn candidates_for_block(&self, block_pos: BlockPos) -> impl Iterator<Item = Entity> {
        let min_x = block_pos.x as f32;
        let min_z = block_pos.z as f32;
        let max_x = min_x + 1.0 - f32::EPSILON;
        let max_z = min_z + 1.0 - f32::EPSILON;

        let cx_min = world_to_chunk_axis(min_x, CHUNK_SIZE_X as i32);
        let cx_max = world_to_chunk_axis(max_x, CHUNK_SIZE_X as i32);
        let cz_min = world_to_chunk_axis(min_z, CHUNK_SIZE_Z as i32);
        let cz_max = world_to_chunk_axis(max_z, CHUNK_SIZE_Z as i32);

        let mut seen: Vec<Entity> = Vec::new();
        for cx in cx_min..=cx_max {
            for cz in cz_min..=cz_max {
                for &entity in self.entities_in_chunk(ChunkPos::new(cx, cz)) {
                    if !seen.contains(&entity) {
                        seen.push(entity);
                    }
                }
            }
        }
        seen.into_iter()
    }
}

// ---------------------------------------------------------------------------
// Chunk footprint calculation
// ---------------------------------------------------------------------------

fn chunks_for_aabb(origin: Vec3, aabb: &Aabb) -> impl Iterator<Item = ChunkPos> {
    let min = aabb.min(origin);
    let max = aabb.max(origin);

    let chunk_x_min = world_to_chunk_axis(min.x, CHUNK_SIZE_X as i32);
    let chunk_x_max = world_to_chunk_axis(max.x - f32::EPSILON, CHUNK_SIZE_X as i32);
    let chunk_z_min = world_to_chunk_axis(min.z, CHUNK_SIZE_Z as i32);
    let chunk_z_max = world_to_chunk_axis(max.z - f32::EPSILON, CHUNK_SIZE_Z as i32);

    let mut result = Vec::with_capacity(4);
    for cx in chunk_x_min..=chunk_x_max {
        for cz in chunk_z_min..=chunk_z_max {
            result.push(ChunkPos::new(cx, cz));
        }
    }
    result.into_iter()
}

#[inline]
fn world_to_chunk_axis(world: f32, chunk_size: i32) -> i32 {
    (world.floor() as i32).div_euclid(chunk_size)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Unit tests for the cache data structure (no Bevy app needed)
    // ------------------------------------------------------------------

    fn player_aabb() -> Aabb {
        Aabb::player()
    }

    #[test]
    fn single_character_well_inside_one_chunk_registers_once() {
        let mut cache = CharacterSpatialCache::default();
        let e = Entity::from_bits(1);
        let aabb = player_aabb();
        cache.rebuild(std::iter::once((e, Vec3::new(4.0, 0.0, 4.0), &aabb)));

        assert_eq!(
            cache.occupied_chunk_count(),
            1,
            "should occupy exactly one chunk"
        );
        assert_eq!(
            cache.registration_count(),
            1,
            "should have one registration"
        );
        assert!(
            cache.entities_in_chunk(ChunkPos::new(0, 0)).contains(&e),
            "entity should be in chunk (0,0)"
        );
    }

    #[test]
    fn character_on_x_chunk_boundary_appears_in_both_chunks() {
        let mut cache = CharacterSpatialCache::default();
        let e = Entity::from_bits(2);
        let aabb = player_aabb();

        cache.rebuild(std::iter::once((e, Vec3::new(16.0, 0.0, 4.0), &aabb)));

        let in_c0 = cache.entities_in_chunk(ChunkPos::new(0, 0)).contains(&e);
        let in_c1 = cache.entities_in_chunk(ChunkPos::new(1, 0)).contains(&e);

        assert!(
            in_c0,
            "entity should appear in chunk (0,0) — its left edge is there"
        );
        assert!(
            in_c1,
            "entity should appear in chunk (1,0) — its right edge is there"
        );
        assert_eq!(cache.registration_count(), 2);
    }

    #[test]
    fn character_on_z_chunk_boundary_appears_in_both_chunks() {
        let mut cache = CharacterSpatialCache::default();
        let e = Entity::from_bits(3);
        let aabb = player_aabb();

        cache.rebuild(std::iter::once((e, Vec3::new(4.0, 0.0, 16.0), &aabb)));

        let in_c0 = cache.entities_in_chunk(ChunkPos::new(0, 0)).contains(&e);
        let in_c1 = cache.entities_in_chunk(ChunkPos::new(0, 1)).contains(&e);

        assert!(in_c0, "entity should appear in chunk (0,0)");
        assert!(in_c1, "entity should appear in chunk (0,1)");
    }

    #[test]
    fn character_on_xz_corner_appears_in_all_four_chunks() {
        let mut cache = CharacterSpatialCache::default();
        let e = Entity::from_bits(4);
        let aabb = player_aabb();

        cache.rebuild(std::iter::once((e, Vec3::new(16.0, 0.0, 16.0), &aabb)));

        for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert!(
                cache.entities_in_chunk(ChunkPos::new(cx, cz)).contains(&e),
                "entity should appear in chunk ({cx},{cz})"
            );
        }
        assert_eq!(cache.registration_count(), 4);
    }

    #[test]
    fn candidate_pairs_returns_each_pair_once() {
        let mut cache = CharacterSpatialCache::default();
        let e1 = Entity::from_bits(10);
        let e2 = Entity::from_bits(11);
        let aabb = player_aabb();

        cache.rebuild(
            [
                (e1, Vec3::new(16.0, 0.0, 4.0), &aabb),
                (e2, Vec3::new(16.0, 0.0, 4.0), &aabb),
            ]
            .into_iter(),
        );

        let pairs: Vec<_> = cache.candidate_pairs().collect();
        assert_eq!(
            pairs.len(),
            1,
            "pair should be emitted exactly once, got {pairs:?}"
        );
    }

    #[test]
    fn two_characters_in_different_chunks_produce_no_pair() {
        let mut cache = CharacterSpatialCache::default();
        let e1 = Entity::from_bits(20);
        let e2 = Entity::from_bits(21);
        let aabb = player_aabb();

        cache.rebuild(
            [
                (e1, Vec3::new(4.0, 0.0, 4.0), &aabb),
                (e2, Vec3::new(84.0, 0.0, 84.0), &aabb),
            ]
            .into_iter(),
        );

        let pairs: Vec<_> = cache.candidate_pairs().collect();
        assert!(
            pairs.is_empty(),
            "characters in different chunks should not be paired: {pairs:?}"
        );
    }

    #[test]
    fn rebuild_clears_previous_state() {
        let mut cache = CharacterSpatialCache::default();
        let old = Entity::from_bits(30);
        let new = Entity::from_bits(31);
        let aabb = player_aabb();

        cache.rebuild(std::iter::once((old, Vec3::new(4.0, 0.0, 4.0), &aabb)));
        assert_eq!(cache.registration_count(), 1);

        cache.rebuild(std::iter::once((new, Vec3::new(4.0, 0.0, 4.0), &aabb)));

        assert_eq!(cache.registration_count(), 1);
        assert!(
            !cache.entities_in_chunk(ChunkPos::new(0, 0)).contains(&old),
            "old entity should have been cleared by rebuild"
        );
        assert!(
            cache.entities_in_chunk(ChunkPos::new(0, 0)).contains(&new),
            "new entity should be present after rebuild"
        );
    }

    #[test]
    fn negative_chunk_coordinates_handled_correctly() {
        let mut cache = CharacterSpatialCache::default();
        let e = Entity::from_bits(40);
        let aabb = player_aabb();

        cache.rebuild(std::iter::once((e, Vec3::new(-4.0, 0.0, -4.0), &aabb)));

        assert!(
            cache.entities_in_chunk(ChunkPos::new(-1, -1)).contains(&e),
            "entity at negative coords should map to chunk (-1,-1)"
        );
    }

    #[test]
    fn empty_rebuild_leaves_cache_empty() {
        let mut cache = CharacterSpatialCache::default();
        cache.rebuild(std::iter::empty());
        assert_eq!(cache.occupied_chunk_count(), 0);
        assert_eq!(cache.registration_count(), 0);
        assert_eq!(cache.candidate_pairs().count(), 0);
    }
}
