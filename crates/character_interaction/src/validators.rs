//! Chunk-change validator systems registered with
//! [`ChunkAuthorityPlugin`](dd40_core::chunk::ChunkAuthorityPlugin).
//!
//! Validators inspect predicted [`ChunkChange`]s before the authority
//! commit pass turns them into confirmed history. Each validator is a
//! regular Bevy system that reads `Res<ChunkCache>` plus whatever else
//! it needs, and writes rejection decisions into
//! [`PendingChunkRejections`] — see the `dd40_core::chunk::authority`
//! module docs for the full design.
//!
//! ## Source-driven decisions
//!
//! - `Res<ChunkCache>` (read-only) + `ResMut<PendingChunkRejections>` is
//!   the canonical validator-system param shape established by
//!   `dd40_core::chunk::authority::default_block_registry_validator`.
//! - `BlockRegistry::collision_shape` is the authoritative way to ask
//!   whether a block has any physical presence — non-collidable blocks
//!   (flowers, torches, …) must never be rejected on collision grounds.
//! - The world-space conversion uses
//!   [`ChunkPos::block_pos`](dd40_core::chunk::ChunkPos::block_pos), the
//!   inverse of `BlockPos::chunk_pos` + `BlockPos::chunk_local`. This is
//!   the only place we go (chunk-local) → (world) inside this crate.

use bevy::prelude::*;
use dd40_core::block::{Block, CollisionShape};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::chunk::{
    ChunkChange, ChunkPos, PendingChunkRejections, RejectReason,
};
use dd40_core::prelude::BlockRegistry;
use dd40_physics_core::prelude::{Aabb, CharacterPosition, CharacterSpatialCache};

/// Reject any predicted [`ChunkChange::Place`] whose target cell would
/// overlap a [`Character`](dd40_character_core::components::Character)'s
/// AABB.
///
/// This is the validator counterpart of the old, soon-to-be-removed
/// server-side `receive_place_requests` collision check in
/// `dd40_network::server::block_placement`. It runs only on instances
/// that have added [`ChunkAuthorityPlugin`](dd40_core::chunk::ChunkAuthorityPlugin)
/// — the server. The client never runs it; client-side prediction
/// optimistically accepts the placement and reconciles when the server's
/// `ChunkUpdate` arrives (Phase 4).
///
/// # Algorithm
///
/// For every dirty chunk, walk its predicted queue and inspect each
/// `Place`:
///
/// 1. Look up the placed block's [`CollisionShape`] via
///    [`BlockRegistry::collision_shape`]. If it is
///    [`CollisionShape::None`] (flowers, torches, …) skip the check —
///    these blocks have no physical presence.
/// 2. Convert the chunk-local coordinate to world space via
///    [`ChunkPos::block_pos`].
/// 3. Use the [`CharacterSpatialCache`] to narrow the candidate set to
///    characters that share a chunk with the target cell, then run a
///    precise [`Aabb::overlaps`] check on each candidate.
/// 4. If any character overlaps the cell's 1×1×1 AABB, write a rejection
///    via [`PendingChunkRejections::reject`].
///
/// `Remove` and `Replace` predictions are ignored — removing a block
/// cannot trap a character, and `Replace` is reserved for non-gameplay
/// callers (world generation, redstone) that already know what they are
/// doing.
pub fn character_collision_validator(
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    spatial_cache: Res<CharacterSpatialCache>,
    characters: Query<(&CharacterPosition, &Aabb)>,
    mut pending: ResMut<PendingChunkRejections>,
) {
    // Snapshot dirty positions so the iteration borrow on the cache is
    // strictly read-only — keeps this system parallel-friendly with
    // anything else that doesn't write the cache.
    let dirty: Vec<ChunkPos> = cache.dirty_chunks().copied().collect();
    for chunk_pos in dirty {
        let Some(chunk) = cache.get(&chunk_pos) else {
            continue;
        };
        for (i, entry) in chunk.predicted().iter().enumerate() {
            let ChunkChange::Place { local, block_id } = &entry.change else {
                continue;
            };

            let placed = Block::new(*block_id);
            if matches!(registry.collision_shape(&placed), CollisionShape::None) {
                continue;
            }

            let world_pos = chunk_pos.block_pos(*local);
            // Block AABB in world space — origin at bottom-centre,
            // half-extents 0.5 for a 1×1×1 cell.
            let block_aabb = Aabb::new(0.5, 0.5, 0.5);
            let block_origin = Vec3::new(
                world_pos.x as f32 + 0.5,
                world_pos.y as f32,
                world_pos.z as f32 + 0.5,
            );

            let overlaps = spatial_cache
                .candidates_for_block(world_pos)
                .any(|entity| match characters.get(entity) {
                    Ok((char_pos, char_aabb)) => {
                        char_aabb.overlaps(char_pos.0, &block_aabb, block_origin)
                    }
                    Err(_) => false,
                });

            if overlaps {
                pending.reject(
                    chunk_pos,
                    i,
                    RejectReason::custom("placement would overlap a character"),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::{BlockDefinition, BlockId, registry::BlockRegistry};
    use dd40_core::chunk::events::ChunkChanged;
    use dd40_core::chunk::{Chunk, ChunkAuthorityPlugin, change::BlockLocal};

    fn registry_with_solid_stone() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_collision_shape(CollisionShape::FullCube),
        );
        // A non-collidable decorative block.
        r.register_without_event(
            BlockDefinition::new(BlockId(2), "flower")
                .with_collision_shape(CollisionShape::None),
        );
        r
    }

    /// Build an app with the authority plugin + collision validator,
    /// pre-populated with one chunk and a character at world position
    /// (0.5, 0.0, 0.5) (centre of cell (0,0,0)).
    fn build_app_with_character() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app.add_message::<ChunkChanged>();
        app.insert_resource(registry_with_solid_stone());
        app.insert_resource(CharacterSpatialCache::default());

        let mut cache = ChunkCache::new();
        cache.insert(Chunk::new(ChunkPos::new(0, 0)));
        app.insert_resource(cache);

        app.add_plugins(ChunkAuthorityPlugin);
        app.add_systems(
            PostUpdate,
            character_collision_validator
                .in_set(dd40_core::chunk::ChunkAuthoritySet::Validate),
        );

        // Spawn a character at the centre of cell (0, 0, 0). The Aabb
        // convention places the origin at bottom-centre.
        let aabb = Aabb::new(0.3, 0.9, 0.3);
        let origin = Vec3::new(0.5, 0.0, 0.5);
        let entity = app
            .world_mut()
            .spawn((CharacterPosition(origin), aabb))
            .id();

        // Rebuild the spatial cache so the validator can find the
        // character via `candidates_for_block`.
        app.world_mut()
            .resource_mut::<CharacterSpatialCache>()
            .rebuild(std::iter::once((entity, origin, &aabb)));

        (app, entity)
    }

    #[test]
    fn rejects_place_inside_character() {
        let (mut app, _e) = build_app_with_character();
        // Predict a Place into the cell the character occupies.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(
                ChunkPos::new(0, 0),
                ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(1)),
            );

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        // Rejected → version stays 0, predicted queue drained, cell
        // remains air (rolled back).
        assert_eq!(chunk.version(), 0);
        assert!(chunk.predicted().is_empty());
        assert_eq!(
            chunk.get_local(BlockLocal::new(0, 0, 0)).block_id,
            BlockId::AIR
        );
    }

    #[test]
    fn allows_place_in_empty_cell() {
        let (mut app, _e) = build_app_with_character();
        // Predict a Place into a cell far from the character.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(
                ChunkPos::new(0, 0),
                ChunkChange::new_place(BlockLocal::new(10, 10, 10), BlockId(1)),
            );

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        assert_eq!(chunk.version(), 1);
        assert_eq!(
            chunk.get_local(BlockLocal::new(10, 10, 10)).block_id,
            BlockId(1)
        );
    }

    #[test]
    fn allows_non_collidable_block_inside_character() {
        let (mut app, _e) = build_app_with_character();
        // Flowers (CollisionShape::None) inside a character are fine.
        app.world_mut()
            .resource_mut::<ChunkCache>()
            .push_predicted(
                ChunkPos::new(0, 0),
                ChunkChange::new_place(BlockLocal::new(0, 0, 0), BlockId(2)),
            );

        app.update();

        let cache = app.world().resource::<ChunkCache>();
        let chunk = cache.get(&ChunkPos::new(0, 0)).unwrap();
        assert_eq!(chunk.version(), 1);
        assert_eq!(
            chunk.get_local(BlockLocal::new(0, 0, 0)).block_id,
            BlockId(2)
        );
    }
}
