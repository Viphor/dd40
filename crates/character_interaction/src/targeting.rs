use bevy::color::palettes::basic::OLIVE;
use bevy::prelude::*;
use dd40_character_core::components::{Character, Player};
use dd40_character_core::face::CharacterFace;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;

// `BlockFace` and `TargetedBlock` were moved to `dd40_character_core` so that
// any Tier-1 crate can read them without depending on `dd40_character_interaction`.
// Re-export them here under their original paths for backwards compatibility.
pub use dd40_character_core::targeted_block::{BlockFace, TargetedBlock};

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for the block-targeting raycast.
///
/// Render-only configuration (highlight colour, mining break colours) lives
/// in `dd40_character_gui::block_highlight::BlockHighlightConfig` so that
/// the headless server never needs to construct it.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct BlockInteractionConfig {
    /// Maximum reach distance in blocks.
    pub max_distance: f32,
}

impl Default for BlockInteractionConfig {
    fn default() -> Self {
        Self { max_distance: 5.0 }
    }
}

// ── DDA raycast ───────────────────────────────────────────────────────────────

fn dda_raycast(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    cache: &ChunkCache,
    registry: &BlockRegistry,
) -> Option<(BlockPos, BlockFace, BlockId)> {
    let mut voxel = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let step = IVec3::new(
        if direction.x >= 0.0 { 1 } else { -1 },
        if direction.y >= 0.0 { 1 } else { -1 },
        if direction.z >= 0.0 { 1 } else { -1 },
    );

    #[derive(Clone, Copy)]
    enum LastAxis { X, Y, Z }
    let mut last_axis = LastAxis::Y;

    let delta = Vec3::new(
        if direction.x != 0.0 { (1.0 / direction.x).abs() } else { f32::INFINITY },
        if direction.y != 0.0 { (1.0 / direction.y).abs() } else { f32::INFINITY },
        if direction.z != 0.0 { (1.0 / direction.z).abs() } else { f32::INFINITY },
    );

    let mut t_max = Vec3::new(
        if direction.x >= 0.0 {
            (voxel.x as f32 + 1.0 - origin.x) / direction.x.abs()
        } else if direction.x < 0.0 {
            (origin.x - voxel.x as f32) / direction.x.abs()
        } else {
            f32::INFINITY
        },
        if direction.y >= 0.0 {
            (voxel.y as f32 + 1.0 - origin.y) / direction.y.abs()
        } else if direction.y < 0.0 {
            (origin.y - voxel.y as f32) / direction.y.abs()
        } else {
            f32::INFINITY
        },
        if direction.z >= 0.0 {
            (voxel.z as f32 + 1.0 - origin.z) / direction.z.abs()
        } else if direction.z < 0.0 {
            (origin.z - voxel.z as f32) / direction.z.abs()
        } else {
            f32::INFINITY
        },
    );

    loop {
        let t_min = t_max.min_element();
        if t_min > max_distance {
            return None;
        }

        let pos = BlockPos::new(voxel.x, voxel.y, voxel.z);
        let chunk_pos = pos.chunk_pos();

        if let Some(chunk) = cache.get(&chunk_pos) {
            let local = pos.chunk_local();
            if let Some(block) =
                chunk.get(local.x as usize, local.y as usize, local.z as usize)
            {
                if block.block_id != BlockId::AIR && registry.is_solid(&block) {
                    let face = match last_axis {
                        LastAxis::X => {
                            if step.x > 0 { BlockFace::West } else { BlockFace::East }
                        }
                        LastAxis::Y => {
                            if step.y > 0 { BlockFace::Bottom } else { BlockFace::Top }
                        }
                        LastAxis::Z => {
                            if step.z > 0 { BlockFace::North } else { BlockFace::South }
                        }
                    };
                    return Some((pos, face, block.block_id));
                }
            }
        }

        if t_max.x < t_max.y && t_max.x < t_max.z {
            voxel.x += step.x;
            t_max.x += delta.x;
            last_axis = LastAxis::X;
        } else if t_max.y < t_max.z {
            voxel.y += step.y;
            t_max.y += delta.y;
            last_axis = LastAxis::Y;
        } else {
            voxel.z += step.z;
            t_max.z += delta.z;
            last_axis = LastAxis::Z;
        }
    }
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Casts a ray from every [`CharacterFace`] and writes the result into the
/// parent [`Character`]'s [`TargetedBlock`] component.
///
/// Runs for **every** character — local players, remote players, NPCs.
/// Each character's face is its own ray origin; the result is written to
/// that character's own `TargetedBlock`.  This lets the same system work
/// on the headless server (which has no [`Player`] marker — that's a
/// local-only concept) for authoritative interaction handling.
///
/// Local-display systems ([`update_debug_info`]) still filter on
/// [`Player`] to render only the local character's target. The targeted-block
/// highlight gizmo lives in `dd40_character_gui::block_highlight`.
pub(crate) fn update_targeted_block(
    config: Res<BlockInteractionConfig>,
    face_query: Query<(&GlobalTransform, &ChildOf), With<CharacterFace>>,
    mut character_query: Query<&mut TargetedBlock, With<Character>>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
) {
    for (face_transform, child_of) in face_query.iter() {
        let Ok(mut targeted) = character_query.get_mut(child_of.parent()) else {
            continue;
        };

        let origin = face_transform.translation();
        let direction = face_transform.forward().as_vec3();

        match dda_raycast(origin, direction, config.max_distance, &cache, &registry) {
            Some((pos, face, block_id)) => {
                targeted.pos = Some(pos);
                targeted.face = Some(face);
                targeted.block_id = Some(block_id);
            }
            None => {
                targeted.pos = None;
                targeted.face = None;
                targeted.block_id = None;
            }
        }
    }
}

#[derive(Component)]
pub(crate) struct TargetedBlockDebugInfo;

pub(crate) fn spawn_debug_entity(mut commands: Commands) {
    commands.spawn((
        Name::new("Block Interaction Debug"),
        DebugInfo::new("Block Interaction Debug Info")
            .with_color(OLIVE.into())
            .add("targeted_block", "Targeted block"),
        TargetedBlockDebugInfo,
    ));
}

pub(crate) fn update_debug_info(
    targeted_query: Query<&TargetedBlock, With<Player>>,
    mut query: Query<&mut DebugInfo, With<TargetedBlockDebugInfo>>,
) {
    let Ok(mut debug_info) = query.single_mut() else { return };
    let Some(targeted) = targeted_query.iter().next() else {
        debug_info.set("targeted_block", "None".to_string());
        return;
    };
    if let Some(pos) = targeted.pos {
        debug_info.set("targeted_block", format!("{:?} at {pos}", targeted.face.unwrap()));
    } else {
        debug_info.set("targeted_block", "None".to_string());
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::{block::{Block, BlockDefinition, BlockId}, chunk::Chunk};

    fn raycast_pos(origin: Vec3, direction: Vec3, max_distance: f32, cache: &ChunkCache, registry: &BlockRegistry) -> Option<BlockPos> {
        dda_raycast(origin, direction, max_distance, cache, registry).map(|(pos, _, _)| pos)
    }

    fn raycast_face(origin: Vec3, direction: Vec3, max_distance: f32, cache: &ChunkCache, registry: &BlockRegistry) -> Option<BlockFace> {
        dda_raycast(origin, direction, max_distance, cache, registry).map(|(_, face, _)| face)
    }

    fn make_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        reg.register_without_event(BlockDefinition::new(BlockId(1), "stone").with_solid(true).with_renderable(true));
        reg
    }

    fn cache_with_block(lx: usize, ly: usize, lz: usize, block: Block) -> ChunkCache {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set(lx, ly, lz, block);
        let mut cache = ChunkCache::new();
        cache.insert(chunk);
        cache
    }

    #[test]
    fn raycast_hits_block_directly_below() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(0.5, 62.0, 0.5), Vec3::NEG_Y, 5.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(0, 60, 0)));
    }

    #[test]
    fn raycast_misses_when_distance_exceeded() {
        let registry = make_registry();
        let cache = cache_with_block(0, 55, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(0.5, 60.0, 0.5), Vec3::NEG_Y, 3.0, &cache, &registry);
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_ignores_air() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId::AIR));
        let hit = raycast_pos(Vec3::new(0.5, 62.0, 0.5), Vec3::NEG_Y, 5.0, &cache, &registry);
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_hits_block_along_x_axis() {
        let registry = make_registry();
        let cache = cache_with_block(5, 64, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(5, 64, 0)));
    }

    #[test]
    fn raycast_zero_direction_returns_none() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(0.5, 62.0, 0.5), Vec3::ZERO, 5.0, &cache, &registry);
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_hits_block_along_diagonal_xz() {
        let registry = make_registry();
        let cache = cache_with_block(3, 64, 3, Block::new(BlockId(1)));
        let direction = Vec3::new(1.0, 0.0, 1.0).normalize();
        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), direction, 6.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(3, 64, 3)));
    }

    #[test]
    fn raycast_skips_unloaded_chunks() {
        let registry = make_registry();
        let cache = ChunkCache::new();
        let hit = raycast_pos(Vec3::new(0.5, 62.0, 0.5), Vec3::NEG_Y, 5.0, &cache, &registry);
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_hits_block_at_origin() {
        let registry = make_registry();
        let cache = cache_with_block(2, 64, 2, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(2.5, 64.5, 2.5), Vec3::X, 5.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(2, 64, 2)));
    }

    #[test]
    fn raycast_zero_max_distance_returns_none() {
        let registry = make_registry();
        let cache = cache_with_block(1, 64, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 0.0, &cache, &registry);
        assert!(hit.is_none());
    }

    #[test]
    fn raycast_hits_block_in_negative_direction() {
        let registry = make_registry();
        let cache = cache_with_block(2, 64, 0, Block::new(BlockId(1)));
        let hit = raycast_pos(Vec3::new(5.5, 64.5, 0.5), Vec3::NEG_X, 5.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(2, 64, 0)));
    }

    #[test]
    fn raycast_crosses_chunk_boundary() {
        let registry = make_registry();
        let chunk_a = Chunk::new(ChunkPos::new(0, 0, 0));
        let mut chunk_b = Chunk::new(ChunkPos::new(1, 0, 0));
        chunk_b.set(2, 64, 0, Block::new(BlockId(1)));
        let mut cache = ChunkCache::new();
        cache.insert(chunk_a);
        cache.insert(chunk_b);
        let hit = raycast_pos(Vec3::new(14.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(18, 64, 0)));
    }

    #[test]
    fn raycast_crosses_y_chunk_boundary_downward() {
        // A ray cast straight down from a point inside chunk (0, 1, 0)
        // must traverse the chunk-Y boundary and hit a block in
        // chunk (0, 0, 0).
        use dd40_core::chunk::CHUNK_SIZE_Y;
        let registry = make_registry();
        let chunk_above = Chunk::new(ChunkPos::new(0, 1, 0));
        let mut chunk_below = Chunk::new(ChunkPos::new(0, 0, 0));
        // Block at world y = CHUNK_SIZE_Y - 4, i.e. local y = CHUNK_SIZE_Y - 4
        // in the lower chunk.
        chunk_below.set(0, CHUNK_SIZE_Y - 4, 0, Block::new(BlockId(1)));
        let mut cache = ChunkCache::new();
        cache.insert(chunk_above);
        cache.insert(chunk_below);

        let origin = Vec3::new(0.5, CHUNK_SIZE_Y as f32 + 5.0, 0.5);
        let hit = raycast_pos(origin, Vec3::NEG_Y, 20.0, &cache, &registry);
        assert_eq!(
            hit,
            Some(BlockPos::new(0, CHUNK_SIZE_Y as i32 - 4, 0)),
            "ray from chunk(0,1,0) should hit block in chunk(0,0,0) below the Y boundary",
        );
    }

    #[test]
    fn raycast_crosses_y_chunk_boundary_upward() {
        // A ray cast straight up from chunk (0, 0, 0) must traverse the
        // chunk-Y boundary and hit a block in chunk (0, 1, 0).
        use dd40_core::chunk::CHUNK_SIZE_Y;
        let registry = make_registry();
        let chunk_below = Chunk::new(ChunkPos::new(0, 0, 0));
        let mut chunk_above = Chunk::new(ChunkPos::new(0, 1, 0));
        // Block at local y = 3 in upper chunk (world y = CHUNK_SIZE_Y + 3).
        chunk_above.set(0, 3, 0, Block::new(BlockId(1)));
        let mut cache = ChunkCache::new();
        cache.insert(chunk_below);
        cache.insert(chunk_above);

        let origin = Vec3::new(0.5, CHUNK_SIZE_Y as f32 - 5.0, 0.5);
        let hit = raycast_pos(origin, Vec3::Y, 20.0, &cache, &registry);
        assert_eq!(
            hit,
            Some(BlockPos::new(0, CHUNK_SIZE_Y as i32 + 3, 0)),
            "ray from chunk(0,0,0) should hit block in chunk(0,1,0) above the Y boundary",
        );
    }

    #[test]
    fn face_top_when_ray_comes_from_above() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(0.5, 62.0, 0.5), Vec3::NEG_Y, 5.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::Top));
    }

    #[test]
    fn face_bottom_when_ray_comes_from_below() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 0, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(0.5, 62.0, 0.5), Vec3::Y, 5.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::Bottom));
    }

    #[test]
    fn face_west_when_ray_travels_positive_x() {
        let registry = make_registry();
        let cache = cache_with_block(5, 64, 0, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::West));
    }

    #[test]
    fn face_east_when_ray_travels_negative_x() {
        let registry = make_registry();
        let cache = cache_with_block(2, 64, 0, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(5.5, 64.5, 0.5), Vec3::NEG_X, 5.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::East));
    }

    #[test]
    fn face_north_when_ray_travels_positive_z() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 5, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(0.5, 64.5, 0.5), Vec3::Z, 10.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::North));
    }

    #[test]
    fn face_south_when_ray_travels_negative_z() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 2, Block::new(BlockId(1)));
        let face = raycast_face(Vec3::new(0.5, 64.5, 5.5), Vec3::NEG_Z, 5.0, &cache, &registry);
        assert_eq!(face, Some(BlockFace::South));
    }

    #[test]
    fn block_face_normals_are_correct() {
        assert_eq!(BlockFace::Top.normal(), BlockPos::new(0, 1, 0));
        assert_eq!(BlockFace::Bottom.normal(), BlockPos::new(0, -1, 0));
        assert_eq!(BlockFace::East.normal(), BlockPos::new(1, 0, 0));
        assert_eq!(BlockFace::West.normal(), BlockPos::new(-1, 0, 0));
        assert_eq!(BlockFace::South.normal(), BlockPos::new(0, 0, 1));
        assert_eq!(BlockFace::North.normal(), BlockPos::new(0, 0, -1));
    }

    /// Server-side scenario: two characters, neither has the (local-only)
    /// `Player` marker. `update_targeted_block` must still write a
    /// per-character `TargetedBlock` for both based on each one's face.
    #[test]
    fn update_targeted_block_runs_for_every_character_without_player_marker() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.insert_resource(BlockInteractionConfig::default());
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(1), "stone").with_solid(true).with_renderable(true),
        );
        app.insert_resource(registry);

        let mut cache = ChunkCache::new();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set(3, 64, 0, Block::new(BlockId(1)));
        chunk.set(0, 64, 3, Block::new(BlockId(1)));
        cache.insert(chunk);
        app.insert_resource(cache);

        // Character A — looks +X toward block (3, 64, 0).
        let a = app
            .world_mut()
            .spawn((Character, TargetedBlock::default()))
            .id();
        let face_a_xform = Transform::from_xyz(0.5, 64.0, 0.5).looking_to(Vec3::X, Vec3::Y);
        app.world_mut().spawn((
            CharacterFace::default(),
            face_a_xform,
            GlobalTransform::from(face_a_xform),
            ChildOf(a),
        ));

        // Character B — looks +Z toward block (0, 64, 3).
        let b = app
            .world_mut()
            .spawn((Character, TargetedBlock::default()))
            .id();
        let face_b_xform = Transform::from_xyz(0.5, 64.0, 0.5).looking_to(Vec3::Z, Vec3::Y);
        app.world_mut().spawn((
            CharacterFace::default(),
            face_b_xform,
            GlobalTransform::from(face_b_xform),
            ChildOf(b),
        ));

        app.world_mut().run_system_once(update_targeted_block).unwrap();

        let a_target = app.world().get::<TargetedBlock>(a).unwrap();
        let b_target = app.world().get::<TargetedBlock>(b).unwrap();
        assert_eq!(a_target.pos, Some(BlockPos::new(3, 64, 0)), "A targets +X block");
        assert_eq!(b_target.pos, Some(BlockPos::new(0, 64, 3)), "B targets +Z block");
    }
}
