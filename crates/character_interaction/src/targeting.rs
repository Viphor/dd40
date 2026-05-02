use bevy::color::palettes::basic::OLIVE;
use bevy::prelude::*;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for the block-targeting raycast.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct BlockInteractionConfig {
    /// Maximum reach distance in blocks.
    pub max_distance: f32,
    /// Color of the wireframe box drawn around the targeted block.
    pub highlight_color: Color,
}

impl Default for BlockInteractionConfig {
    fn default() -> Self {
        Self {
            max_distance: 5.0,
            highlight_color: Color::BLACK,
        }
    }
}

// ── Block face ────────────────────────────────────────────────────────────────

/// The face of a block that the crosshair ray entered from.
///
/// # Placement offset
///
/// ```
/// use dd40_character_interaction::targeting::BlockFace;
/// use dd40_core::prelude::BlockPos;
///
/// let hit_pos = BlockPos::new(3, 64, 5);
/// let face    = BlockFace::Top;
/// let place_pos = BlockPos::new(
///     hit_pos.x + face.normal().x,
///     hit_pos.y + face.normal().y,
///     hit_pos.z + face.normal().z,
/// );
/// assert_eq!(place_pos, BlockPos::new(3, 65, 5));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum BlockFace {
    /// The +Y face (ray came from above).
    Top,
    /// The -Y face (ray came from below).
    Bottom,
    /// The +X face.
    East,
    /// The -X face.
    West,
    /// The +Z face.
    South,
    /// The -Z face.
    North,
}

impl BlockFace {
    /// Returns the unit offset to add to the hit block's [`BlockPos`] to get
    /// the face-adjacent voxel (where a new block would be placed).
    pub fn normal(self) -> BlockPos {
        match self {
            BlockFace::Top => BlockPos::new(0, 1, 0),
            BlockFace::Bottom => BlockPos::new(0, -1, 0),
            BlockFace::East => BlockPos::new(1, 0, 0),
            BlockFace::West => BlockPos::new(-1, 0, 0),
            BlockFace::South => BlockPos::new(0, 0, 1),
            BlockFace::North => BlockPos::new(0, 0, -1),
        }
    }
}

// ── Targeted-block state ───────────────────────────────────────────────────────

/// The block the character is currently looking at, if any.
///
/// Updated every frame by [`update_targeted_block`].
#[derive(Resource, Debug, Default, Clone, Reflect)]
#[reflect(Resource)]
pub struct TargetedBlock {
    pub pos: Option<BlockPos>,
    pub face: Option<BlockFace>,
    pub block_id: Option<BlockId>,
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
            if local.y >= 0 {
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

/// Casts a ray from the camera and writes the result into [`TargetedBlock`].
pub(crate) fn update_targeted_block(
    mut targeted: ResMut<TargetedBlock>,
    config: Res<BlockInteractionConfig>,
    camera_query: Query<&Transform, With<Camera3d>>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        targeted.pos = None;
        targeted.face = None;
        targeted.block_id = None;
        return;
    };

    let origin = camera_transform.translation;
    let direction = *camera_transform.forward();

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

/// Draws a wireframe cuboid gizmo around the currently targeted block.
pub(crate) fn draw_targeted_block_highlight(
    targeted: Res<TargetedBlock>,
    config: Res<BlockInteractionConfig>,
    mut gizmos: Gizmos,
) {
    let Some(pos) = targeted.pos else { return };
    let center = Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5);
    const EPSILON: f32 = 0.0002;
    let size = Vec3::splat(1.0 + EPSILON * 2.0);
    gizmos.cube(Transform::from_translation(center).with_scale(size), config.highlight_color);
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
    targeted: Res<TargetedBlock>,
    mut query: Query<&mut DebugInfo, With<TargetedBlockDebugInfo>>,
) {
    let Ok(mut debug_info) = query.single_mut() else { return };
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
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
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
        let chunk_a = Chunk::new(ChunkPos::new(0, 0));
        let mut chunk_b = Chunk::new(ChunkPos::new(1, 0));
        chunk_b.set(2, 64, 0, Block::new(BlockId(1)));
        let mut cache = ChunkCache::new();
        cache.insert(chunk_a);
        cache.insert(chunk_b);
        let hit = raycast_pos(Vec3::new(14.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);
        assert_eq!(hit, Some(BlockPos::new(18, 64, 0)));
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
}
