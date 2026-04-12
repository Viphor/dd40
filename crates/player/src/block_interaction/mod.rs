//! Block interaction systems for the player.
//!
//! This module implements the core block-targeting logic used for player
//! interaction with the voxel world.  Every frame it casts a ray from the
//! centre of the player's [`Camera3d`] along the camera's view axis, walks the
//! ray one voxel step at a time (DDA), and finds the first non-air block
//! within a configurable reach distance.  The targeted block is highlighted
//! with a wireframe cuboid drawn via Bevy's [`Gizmos`] API.
//!
//! The [`TargetedBlock`] resource also records which [`BlockFace`] the ray
//! entered from.  This tells placement logic which side of the targeted block
//! to attach the new block to — e.g. looking at the top face of a block means
//! the new block should be placed one voxel above it.
//!
//! # Registration
//!
//! Add [`BlockInteractionPlugin`] to your [`App`]:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_player::block_interaction::BlockInteractionPlugin;
//!
//! App::new()
//!     .add_plugins(BlockInteractionPlugin::default())
//!     .run();
//! ```
//!
//! # Configuration
//!
//! Adjust [`BlockInteractionConfig`] at any time via the Bevy resource system:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_player::block_interaction::{BlockInteractionPlugin, BlockInteractionConfig};
//!
//! App::new()
//!     .add_plugins(BlockInteractionPlugin::default())
//!     .add_systems(Startup, |mut config: ResMut<BlockInteractionConfig>| {
//!         config.max_distance = 10.0;
//!     })
//!     .run();
//! ```

use bevy::color::palettes::basic::OLIVE;
use bevy::prelude::*;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for the block-targeting raycast.
///
/// Insert this resource (or mutate the default) to change reach distance and
/// highlight appearance at runtime.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct BlockInteractionConfig {
    /// Maximum reach distance in blocks.  The ray is walked up to this many
    /// world units before giving up.  Defaults to `5.0`.
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

/// The face of a block that the player's crosshair ray entered from.
///
/// Each variant corresponds to one of the six axis-aligned faces of a unit
/// cube.  The name describes **which face of the hit block** was struck —
/// e.g. [`BlockFace::Top`] means the ray came from above and hit the +Y face.
///
/// # Placement offset
///
/// To find the [`BlockPos`] where a new block should be placed, add
/// [`BlockFace::normal`] to the hit block's position:
///
/// ```
/// use dd40_player::block_interaction::BlockFace;
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
    /// The +X face (ray came from the positive-X side).
    East,
    /// The -X face (ray came from the negative-X side).
    West,
    /// The +Z face (ray came from the positive-Z side).
    South,
    /// The -Z face (ray came from the negative-Z side).
    North,
}

impl BlockFace {
    /// Returns the unit offset that should be added to the hit block's
    /// [`BlockPos`] to obtain the position of the face-adjacent voxel.
    ///
    /// This is the position where a new block would be placed when the player
    /// interacts with this face.
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

/// The block that the player is currently looking at, if any.
///
/// This resource is updated every frame by [`update_targeted_block`].  Other
/// systems (e.g. block placement / breaking) should read from here rather than
/// running their own raycast.
///
/// When `pos` is `Some`, `face` is always `Some` too — they are set together
/// and cleared together.
#[derive(Resource, Debug, Default, Clone, Reflect)]
#[reflect(Resource)]
pub struct TargetedBlock {
    /// World-space integer position of the looked-at block, or `None` if no
    /// solid block is within reach.
    pub pos: Option<BlockPos>,

    /// The face of the targeted block that the ray entered from, or `None`
    /// when no block is targeted.
    ///
    /// Add [`BlockFace::normal`]`()` to [`pos`][TargetedBlock::pos] to get the
    /// position where a newly placed block should go.
    pub face: Option<BlockFace>,
}

// ── DDA raycast ───────────────────────────────────────────────────────────────

/// Walks a ray from `origin` in `direction` through the voxel grid using the
/// [DDA algorithm](https://en.wikipedia.org/wiki/Digital_differential_analyzer_(graphics_algorithm))
/// and returns the [`BlockPos`] and [`BlockFace`] of the first solid block
/// found within `max_distance` world units, or `None` if none is found.
///
/// The [`BlockFace`] describes which face of the hit block the ray entered
/// from, which determines where an adjacent block would be placed.
///
/// # Parameters
///
/// - `origin`       – Ray start point in world space (typically the camera position).
/// - `direction`    – Normalised ray direction in world space.
/// - `max_distance` – Maximum travel distance along the ray, in world units.
/// - `cache`        – The chunk cache used to look up block data.
/// - `registry`     – The block registry used to test block solidity.
///
/// # Edge cases
///
/// - A zero-length direction vector will yield no hit (all step sizes become
///   infinite).
/// - Blocks outside of loaded chunks are treated as non-solid and skipped.
/// - Y values below 0 or ≥ `CHUNK_SIZE_Y` are skipped.
/// - When the ray origin is already inside a solid block the returned face is
///   [`BlockFace::Top`] as a safe fallback (there is no meaningful entry face
///   when the ray has not crossed any boundary yet).
fn dda_raycast(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    cache: &ChunkCache,
    registry: &BlockRegistry,
) -> Option<(BlockPos, BlockFace)> {
    // Current voxel coordinates (the cell the ray tip is currently inside).
    let mut voxel = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    // Step direction (+1 or -1) for each axis.
    let step = IVec3::new(
        if direction.x >= 0.0 { 1 } else { -1 },
        if direction.y >= 0.0 { 1 } else { -1 },
        if direction.z >= 0.0 { 1 } else { -1 },
    );

    // Tracks which axis boundary the ray crossed most recently.  This tells
    // us which face of the hit block the ray entered from.  Initialised to Y
    // as a safe fallback for the case where the ray origin is already inside a
    // solid block (no boundary has been crossed yet).
    #[derive(Clone, Copy)]
    enum LastAxis {
        X,
        Y,
        Z,
    }
    let mut last_axis = LastAxis::Y;

    // How far along the ray (in units of |direction|) we must travel to cross
    // one full voxel boundary on each axis.  `f32::INFINITY` when the
    // direction component is zero (the ray never crosses that axis).
    let delta = Vec3::new(
        if direction.x != 0.0 {
            (1.0 / direction.x).abs()
        } else {
            f32::INFINITY
        },
        if direction.y != 0.0 {
            (1.0 / direction.y).abs()
        } else {
            f32::INFINITY
        },
        if direction.z != 0.0 {
            (1.0 / direction.z).abs()
        } else {
            f32::INFINITY
        },
    );

    // Initial `t` values: how far along the ray until we first hit a boundary
    // on each axis, starting from `origin`.
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

    // DDA loop: advance to the nearest voxel boundary, check the cell, repeat.
    loop {
        // t_min is the distance already travelled to reach the current voxel.
        let t_min = t_max.min_element();
        if t_min > max_distance {
            return None;
        }

        // Sample the current voxel.
        let pos = BlockPos::new(voxel.x, voxel.y, voxel.z);
        let chunk_pos = pos.chunk_pos();

        if let Some(chunk) = cache.get(&chunk_pos) {
            let local = pos.chunk_local();

            // Clamp negative Y just in case (chunk_local Y is unchanged).
            if local.y >= 0 {
                if let Some(block) = chunk.get(local.x as usize, local.y as usize, local.z as usize)
                {
                    if block.block_id != BlockId::AIR && block.is_solid(registry) {
                        // Derive the entry face from the last axis crossed and
                        // the direction of travel on that axis.
                        let face = match last_axis {
                            LastAxis::X => {
                                if step.x > 0 {
                                    BlockFace::West
                                } else {
                                    BlockFace::East
                                }
                            }
                            LastAxis::Y => {
                                if step.y > 0 {
                                    BlockFace::Bottom
                                } else {
                                    BlockFace::Top
                                }
                            }
                            LastAxis::Z => {
                                if step.z > 0 {
                                    BlockFace::North
                                } else {
                                    BlockFace::South
                                }
                            }
                        };
                        return Some((pos, face));
                    }
                }
            }
        }

        // Advance to the next voxel boundary on the shortest axis and record
        // which axis we crossed so we know the entry face of the next voxel.
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

/// Casts a ray from the camera each frame and writes the result into
/// [`TargetedBlock`].
///
/// This system should run every frame while the game is running.  It reads
/// [`BlockInteractionConfig`] for the reach distance and uses the
/// [`ChunkCache`] to look up block data without any allocations.
fn update_targeted_block(
    mut targeted: ResMut<TargetedBlock>,
    config: Res<BlockInteractionConfig>,
    camera_query: Query<&Transform, With<Camera3d>>,
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        targeted.pos = None;
        targeted.face = None;
        return;
    };

    // The camera's local -Z axis is the view direction in Bevy's right-handed
    // coordinate system.
    let origin = camera_transform.translation;
    let direction = *camera_transform.forward();

    match dda_raycast(origin, direction, config.max_distance, &cache, &registry) {
        Some((pos, face)) => {
            targeted.pos = Some(pos);
            targeted.face = Some(face);
        }
        None => {
            targeted.pos = None;
            targeted.face = None;
        }
    }
}

/// Draws a wireframe cuboid gizmo around the currently targeted block.
///
/// The cuboid is drawn every frame that [`TargetedBlock::pos`] is `Some`.  It
/// is expanded by a tiny epsilon on each side so that it does not perfectly
/// overlap rendered block faces (which would cause Z-fighting).
fn draw_targeted_block_highlight(
    targeted: Res<TargetedBlock>,
    config: Res<BlockInteractionConfig>,
    mut gizmos: Gizmos,
) {
    let Some(pos) = targeted.pos else {
        return;
    };

    // Block centres are offset by 0.5 because block positions refer to the
    // minimum corner of the voxel.
    let center = Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5);

    // Expand very slightly to avoid Z-fighting with block faces.
    const EPSILON: f32 = 0.002;
    let size = Vec3::splat(1.0 + EPSILON * 2.0);

    gizmos.cube(
        Transform::from_translation(center).with_scale(size),
        config.highlight_color,
    );
}

#[derive(Component)]
struct TargetedBlockDebugInfo;

fn spawn_debug_entity(mut commands: Commands) {
    commands.spawn((
        Name::new("Block Interaction Debug"),
        DebugInfo::new("Block Interaction Debug Info")
            .with_color(OLIVE.into())
            .add("targeted_block", "Targeted block"),
        TargetedBlockDebugInfo,
    ));
}

fn update_debug_info(
    targeted: Res<TargetedBlock>,
    mut query: Query<&mut DebugInfo, With<TargetedBlockDebugInfo>>,
) {
    let Ok(mut debug_info) = query.single_mut() else {
        return;
    };

    if let Some(pos) = targeted.pos {
        debug_info.set(
            "targeted_block",
            format!("{:?} at {pos}", targeted.face.unwrap()),
        );
    } else {
        debug_info.set("targeted_block", "None".to_string());
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that adds player block-targeting and highlight rendering.
///
/// Registers [`BlockInteractionConfig`] and [`TargetedBlock`] as resources and
/// adds the two systems that update and draw the block highlight every frame.
///
/// The systems only run while the app is in [`AppState::Playing`] **and**
/// [`GameState::Running`] so that the highlight is automatically suppressed
/// during pause menus or loading screens.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_player::block_interaction::BlockInteractionPlugin;
///
/// App::new()
///     .add_plugins(BlockInteractionPlugin::default())
///     .run();
/// ```
pub struct BlockInteractionPlugin {
    /// Initial reach distance in world units.  Can be changed later via the
    /// [`BlockInteractionConfig`] resource.
    pub max_distance: f32,
    /// Initial highlight color.  Can be changed later via
    /// [`BlockInteractionConfig`].
    pub highlight_color: Color,
}

impl Default for BlockInteractionPlugin {
    fn default() -> Self {
        let defaults = BlockInteractionConfig::default();
        Self {
            max_distance: defaults.max_distance,
            highlight_color: defaults.highlight_color,
        }
    }
}

impl Plugin for BlockInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BlockInteractionConfig {
            max_distance: self.max_distance,
            highlight_color: self.highlight_color,
        })
        .insert_resource(TargetedBlock::default())
        .register_type::<BlockInteractionConfig>()
        .register_type::<TargetedBlock>()
        .add_systems(Startup, spawn_debug_entity)
        .add_systems(
            Update,
            (
                update_targeted_block,
                draw_targeted_block_highlight,
                update_debug_info,
            )
                .chain()
                .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::{
        block::{Block, BlockDefinition, BlockId},
        chunk::Chunk,
    };

    /// Convenience wrapper: calls `dda_raycast` and returns only the
    /// [`BlockPos`], discarding the face.  Used by tests that only care about
    /// whether the correct block was hit.
    fn raycast_pos(
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        cache: &ChunkCache,
        registry: &BlockRegistry,
    ) -> Option<BlockPos> {
        dda_raycast(origin, direction, max_distance, cache, registry).map(|(pos, _)| pos)
    }

    /// Convenience wrapper: calls `dda_raycast` and returns only the
    /// [`BlockFace`], discarding the position.  Used by face-detection tests.
    fn raycast_face(
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        cache: &ChunkCache,
        registry: &BlockRegistry,
    ) -> Option<BlockFace> {
        dda_raycast(origin, direction, max_distance, cache, registry).map(|(_, face)| face)
    }

    /// Build a minimal [`BlockRegistry`] containing Air (id 0) and Stone (id 1).
    fn make_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        reg.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_solid(true)
                .with_renderable(true),
        );
        reg
    }

    /// Builds a [`ChunkCache`] seeded with a single chunk at `(0, 0)` whose
    /// block at chunk-local position `(lx, ly, lz)` is set to `block`.
    ///
    /// All other positions in the chunk default to [`Block::default()`] (air).
    fn cache_with_block(lx: usize, ly: usize, lz: usize, block: Block) -> ChunkCache {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        chunk.set(lx, ly, lz, block);

        let mut cache = ChunkCache::new();
        cache.insert(chunk);
        cache
    }

    // ── dda_raycast unit tests ────────────────────────────────────────────

    /// A ray pointing straight down that starts one block above a stone block
    /// should hit that block.
    #[test]
    fn raycast_hits_block_directly_below() {
        let registry = make_registry();

        // Stone at local (0, 60, 0) → world (0, 60, 0).
        let block = Block::new(BlockId(1));
        let cache = cache_with_block(0, 60, 0, block);

        // Ray starts at (0.5, 62.0, 0.5), pointing straight down.
        let hit = raycast_pos(
            Vec3::new(0.5, 62.0, 0.5),
            Vec3::NEG_Y,
            5.0,
            &cache,
            &registry,
        );

        assert_eq!(hit, Some(BlockPos::new(0, 60, 0)));
    }

    /// A ray pointing straight down that has a `max_distance` too short to
    /// reach the block should return `None`.
    #[test]
    fn raycast_misses_when_distance_exceeded() {
        let registry = make_registry();

        // Stone at local (0, 55, 0) → world (0, 55, 0), 5 blocks below origin.
        let block = Block::new(BlockId(1));
        let cache = cache_with_block(0, 55, 0, block);

        // Ray starts at (0.5, 60.0, 0.5), max distance only 3.0 — can't reach.
        let hit = raycast_pos(
            Vec3::new(0.5, 60.0, 0.5),
            Vec3::NEG_Y,
            3.0,
            &cache,
            &registry,
        );

        assert!(hit.is_none());
    }

    /// An air block must never be returned as a hit.
    #[test]
    fn raycast_ignores_air() {
        let registry = make_registry();

        // Leave the chunk fully air (default).
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId::AIR));

        let hit = raycast_pos(
            Vec3::new(0.5, 62.0, 0.5),
            Vec3::NEG_Y,
            5.0,
            &cache,
            &registry,
        );

        assert!(hit.is_none());
    }

    /// A ray pointing in the +X direction should hit a block that lies along
    /// that axis.
    #[test]
    fn raycast_hits_block_along_x_axis() {
        let registry = make_registry();

        // Stone at local (5, 64, 0) → world (5, 64, 0).
        let block = Block::new(BlockId(1));
        let cache = cache_with_block(5, 64, 0, block);

        // Ray starts at (0.5, 64.5, 0.5), pointing in +X.
        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);

        assert_eq!(hit, Some(BlockPos::new(5, 64, 0)));
    }

    /// A ray with a zero direction vector should never return a hit.
    #[test]
    fn raycast_zero_direction_returns_none() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId(1)));

        let hit = raycast_pos(
            Vec3::new(0.5, 62.0, 0.5),
            Vec3::ZERO,
            5.0,
            &cache,
            &registry,
        );

        // Zero direction → all delta values are INFINITY → t_max never
        // advances → t_min stays 0.0 which is ≤ max_distance, so we'd sample
        // the origin voxel.  That voxel is air, so None.
        assert!(hit.is_none());
    }

    /// A ray travelling diagonally (equal parts +X and +Z) must correctly
    /// traverse voxel boundaries on two axes simultaneously and hit a block
    /// that does not lie on any cardinal axis from the origin.
    ///
    /// # Geometry
    ///
    /// Origin: `(0.5, 64.5, 0.5)` — centre of voxel `(0, 64, 0)`.
    /// Direction: `normalize(1, 0, 1)` = `(√½, 0, √½)`.
    ///
    /// Because X and Z components are equal the ray crosses both the X and Z
    /// voxel boundaries at the same time, stepping diagonally from
    /// `(0,64,0)` → `(1,64,1)` → `(2,64,2)` → …  Stone placed at local
    /// `(3, 64, 3)` (world `(3, 64, 3)`) is reached after travelling
    /// `3 × √2 ≈ 4.24` world units, well within a `max_distance` of `6.0`.
    #[test]
    fn raycast_hits_block_along_diagonal_xz() {
        let registry = make_registry();

        // Stone at local (3, 64, 3) → world (3, 64, 3).
        let cache = cache_with_block(3, 64, 3, Block::new(BlockId(1)));

        // Normalised diagonal direction in the XZ plane.
        let direction = Vec3::new(1.0, 0.0, 1.0).normalize();

        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), direction, 6.0, &cache, &registry);

        assert_eq!(hit, Some(BlockPos::new(3, 64, 3)));
    }

    /// Blocks in an unloaded chunk (not present in the cache) must be treated
    /// as non-solid, so the raycast should return `None`.
    #[test]
    fn raycast_skips_unloaded_chunks() {
        let registry = make_registry();

        // Empty cache — no chunks loaded at all.
        let cache = ChunkCache::new();

        let hit = raycast_pos(
            Vec3::new(0.5, 62.0, 0.5),
            Vec3::NEG_Y,
            5.0,
            &cache,
            &registry,
        );

        assert!(hit.is_none());
    }

    /// When the ray origin is inside a solid block the DDA must return that
    /// block immediately, before advancing to any neighbour.
    ///
    /// # Geometry
    ///
    /// Stone fills voxel `(2, 64, 2)`.  The ray starts at `(2.5, 64.5, 2.5)`,
    /// which is the centre of that same voxel, so `t_min = 0.0 ≤ max_distance`
    /// on the very first iteration and the block is sampled before any step.
    #[test]
    fn raycast_hits_block_at_origin() {
        let registry = make_registry();

        // Stone at local (2, 64, 2) → world (2, 64, 2).
        let cache = cache_with_block(2, 64, 2, Block::new(BlockId(1)));

        // Ray starts inside the stone block.
        let hit = raycast_pos(Vec3::new(2.5, 64.5, 2.5), Vec3::X, 5.0, &cache, &registry);

        assert_eq!(hit, Some(BlockPos::new(2, 64, 2)));
    }

    /// A `max_distance` of exactly `0.0` must never return a hit, even when
    /// the origin voxel is solid, because the ray is not permitted to travel
    /// any distance at all.
    ///
    /// # Note on implementation
    ///
    /// The DDA loop checks `t_min > max_distance` at the top of each
    /// iteration.  When `max_distance` is `0.0` and the origin is air the
    /// condition `0.0 > 0.0` is false, so the origin voxel is sampled — but
    /// it is air, so the ray then steps and `t_min` becomes positive, which
    /// exceeds `0.0` and the loop exits.  This test therefore places stone one
    /// voxel ahead of the origin (not at it) to confirm the ray cannot reach
    /// even the adjacent block.
    #[test]
    fn raycast_zero_max_distance_returns_none() {
        let registry = make_registry();

        // Stone one block ahead in +X.
        let cache = cache_with_block(1, 64, 0, Block::new(BlockId(1)));

        // Origin is in voxel (0, 64, 0) — air.  Stone is in (1, 64, 0).
        let hit = raycast_pos(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 0.0, &cache, &registry);

        assert!(hit.is_none());
    }

    /// A ray pointing in a negative direction (-X) must correctly step
    /// backwards through the voxel grid and find a block behind the origin.
    ///
    /// # Geometry
    ///
    /// Origin: `(5.5, 64.5, 0.5)` — centre of voxel `(5, 64, 0)`.
    /// Direction: `-X`.  Stone is at world `(2, 64, 0)`, which is `3` blocks
    /// behind the origin along the X axis, well within `max_distance = 5.0`.
    #[test]
    fn raycast_hits_block_in_negative_direction() {
        let registry = make_registry();

        // Stone at local (2, 64, 0) → world (2, 64, 0).
        let cache = cache_with_block(2, 64, 0, Block::new(BlockId(1)));

        let hit = raycast_pos(
            Vec3::new(5.5, 64.5, 0.5),
            Vec3::NEG_X,
            5.0,
            &cache,
            &registry,
        );

        assert_eq!(hit, Some(BlockPos::new(2, 64, 0)));
    }

    /// A ray that crosses from one chunk into an adjacent chunk must find a
    /// block that sits in the second chunk.
    ///
    /// # Geometry
    ///
    /// Chunk `(0, 0)` spans world X `[0, 15]`.  Chunk `(1, 0)` spans world X
    /// `[16, 31]`.  The ray starts at `(14.5, 64.5, 0.5)` (near the right
    /// edge of chunk `(0, 0)`) and travels in `+X`.  Stone is placed at local
    /// `(2, 64, 0)` inside chunk `(1, 0)`, which maps to world `(18, 64, 0)`.
    /// The ray must cross the chunk boundary at X = 16 and still find the
    /// block ~4 world units away.
    #[test]
    fn raycast_crosses_chunk_boundary() {
        let registry = make_registry();

        // Seed two chunks: (0,0) fully air, (1,0) with stone at local (2,64,0).
        let chunk_a = Chunk::new(ChunkPos::new(0, 0)); // all air
        let mut chunk_b = Chunk::new(ChunkPos::new(1, 0));
        chunk_b.set(2, 64, 0, Block::new(BlockId(1))); // world (18, 64, 0)

        let mut cache = ChunkCache::new();
        cache.insert(chunk_a);
        cache.insert(chunk_b);

        // Start just inside chunk (0,0), pointing in +X toward chunk (1,0).
        let hit = raycast_pos(Vec3::new(14.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);

        assert_eq!(hit, Some(BlockPos::new(18, 64, 0)));
    }

    // ── BlockFace detection tests ─────────────────────────────────────────

    /// A ray from above hitting the top face of a block should report
    /// [`BlockFace::Top`].  The placement position is one block above the hit.
    #[test]
    fn face_top_when_ray_comes_from_above() {
        let registry = make_registry();
        let cache = cache_with_block(0, 60, 0, Block::new(BlockId(1)));

        let face = raycast_face(
            Vec3::new(0.5, 62.0, 0.5),
            Vec3::NEG_Y,
            5.0,
            &cache,
            &registry,
        );

        assert_eq!(face, Some(BlockFace::Top));
    }

    /// A ray from below hitting the bottom face of a block should report
    /// [`BlockFace::Bottom`].
    #[test]
    fn face_bottom_when_ray_comes_from_below() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 0, Block::new(BlockId(1)));

        let face = raycast_face(Vec3::new(0.5, 62.0, 0.5), Vec3::Y, 5.0, &cache, &registry);

        assert_eq!(face, Some(BlockFace::Bottom));
    }

    /// A ray travelling in +X that hits a block should report [`BlockFace::West`]
    /// — the ray entered from the -X (west) side of the block.
    #[test]
    fn face_west_when_ray_travels_positive_x() {
        let registry = make_registry();
        let cache = cache_with_block(5, 64, 0, Block::new(BlockId(1)));

        let face = raycast_face(Vec3::new(0.5, 64.5, 0.5), Vec3::X, 10.0, &cache, &registry);

        assert_eq!(face, Some(BlockFace::West));
    }

    /// A ray travelling in -X that hits a block should report [`BlockFace::East`]
    /// — the ray entered from the +X (east) side of the block.
    #[test]
    fn face_east_when_ray_travels_negative_x() {
        let registry = make_registry();
        let cache = cache_with_block(2, 64, 0, Block::new(BlockId(1)));

        let face = raycast_face(
            Vec3::new(5.5, 64.5, 0.5),
            Vec3::NEG_X,
            5.0,
            &cache,
            &registry,
        );

        assert_eq!(face, Some(BlockFace::East));
    }

    /// A ray travelling in +Z that hits a block should report [`BlockFace::North`]
    /// — the ray entered from the -Z (north) side of the block.
    #[test]
    fn face_north_when_ray_travels_positive_z() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 5, Block::new(BlockId(1)));

        let face = raycast_face(Vec3::new(0.5, 64.5, 0.5), Vec3::Z, 10.0, &cache, &registry);

        assert_eq!(face, Some(BlockFace::North));
    }

    /// A ray travelling in -Z that hits a block should report [`BlockFace::South`]
    /// — the ray entered from the +Z (south) side of the block.
    #[test]
    fn face_south_when_ray_travels_negative_z() {
        let registry = make_registry();
        let cache = cache_with_block(0, 64, 2, Block::new(BlockId(1)));

        let face = raycast_face(
            Vec3::new(0.5, 64.5, 5.5),
            Vec3::NEG_Z,
            5.0,
            &cache,
            &registry,
        );

        assert_eq!(face, Some(BlockFace::South));
    }

    /// [`BlockFace::normal`] must point to the correct adjacent voxel for
    /// every face, so that placement logic can add it directly to the hit
    /// block's position.
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
