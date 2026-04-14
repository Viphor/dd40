//! Block-grid collision detection and resolution.
//!
//! # Algorithm
//!
//! Because every block occupies exactly one 1×1×1 unit cell we can resolve
//! collisions in **O(1) per axis** rather than building a broad-phase pipeline:
//!
//! 1. From the entity's [`Aabb`] and [`TentativePosition`] compute the set of
//!    integer block coordinates the AABB *could* overlap (a small bounding
//!    box in block-integer space, typically 2–4 cells per axis).
//! 2. For **each axis independently** (Y first so the grounded flag is correct,
//!    then X, then Z) sweep the AABB from its current position toward the
//!    tentative position, find the nearest blocking cell along that axis, and
//!    stop there.
//! 3. Look up each candidate block in [`ChunkCache`] in O(1) (hash-map
//!    lookup by [`ChunkPos`] then array-indexed local lookup).
//! 4. Check the registered [`CollisionShape`] for that block (falling back to
//!    [`CollisionShape::FullCube`] for solid blocks and [`CollisionShape::None`]
//!    for non-solid blocks).
//!
//! Sweeping each axis independently rather than all three at once avoids the
//! "corner-clip" artifact common in simple overlap-and-push approaches, while
//! keeping the code simple enough to audit at a glance.
//!
//! # Future irregular shapes
//!
//! When a block has a [`CollisionShape::Box`] registered in
//! [`BlockCollisionShapes`], the cell-local AABB is transformed to world space
//! and substituted for the default unit-cube face positions.  This means stairs,
//! slabs, and lecterns only need a shape registration — no changes to this
//! module are required.

use bevy::prelude::*;

use crate::{
    block::registry::BlockRegistry,
    block::{Block, BlockPos},
    character::physics::{
        Aabb, CollisionShape, Grounded, PhysicsBody, PhysicsSet, TentativePosition, Velocity,
    },
    chunk::{CHUNK_SIZE_Y, cache::ChunkCache},
};

// ---------------------------------------------------------------------------
// Collision shape resolution
// ---------------------------------------------------------------------------

/// The world-space AABB of a single block cell used during sweeping.
///
/// All fields are world-space coordinates (not half-extents).
#[derive(Debug, Clone, Copy)]
struct BlockAabb {
    min: Vec3,
    max: Vec3,
}

impl BlockAabb {
    /// Returns `true` when this block AABB overlaps `entity_min..entity_max`
    /// on the two axes *other* than `sweep_axis`.
    ///
    /// We test the "cross-section" of the entity on the non-swept axes to
    /// avoid resolving collisions with blocks that are only grazed in the
    /// perpendicular directions.
    fn overlaps_cross_section(&self, entity_min: Vec3, entity_max: Vec3, sweep_axis: Axis) -> bool {
        match sweep_axis {
            Axis::X => {
                entity_min.y < self.max.y
                    && entity_max.y > self.min.y
                    && entity_min.z < self.max.z
                    && entity_max.z > self.min.z
            }
            Axis::Y => {
                entity_min.x < self.max.x
                    && entity_max.x > self.min.x
                    && entity_min.z < self.max.z
                    && entity_max.z > self.min.z
            }
            Axis::Z => {
                entity_min.x < self.max.x
                    && entity_max.x > self.min.x
                    && entity_min.y < self.max.y
                    && entity_max.y > self.min.y
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

/// Resolves the world-space AABB for a block at integer position `pos`,
/// taking its [`CollisionShape`] from the [`BlockRegistry`] into account.
///
/// Returns `None` when the block has no collision (air, [`CollisionShape::None`]).
fn block_world_aabb(pos: BlockPos, block: Block, registry: &BlockRegistry) -> Option<BlockAabb> {
    // Read the collision shape directly from the registry — it is the single
    // source of truth for all block properties.
    let shape = registry.collision_shape(&block);

    // Cell origin in world space (minimum corner of the 1×1×1 cell).
    let cell_origin = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);

    match shape {
        CollisionShape::None => None,
        CollisionShape::FullCube => Some(BlockAabb {
            min: cell_origin,
            max: cell_origin + Vec3::ONE,
        }),
        CollisionShape::Box { min, max } => {
            // Clamp to [0,1] to ensure the shape stays within the cell.
            let cmin = min.clamp(Vec3::ZERO, Vec3::ONE);
            let cmax = max.clamp(Vec3::ZERO, Vec3::ONE);
            if cmin.x >= cmax.x || cmin.y >= cmax.y || cmin.z >= cmax.z {
                // Degenerate shape — treat as no collision.
                None
            } else {
                Some(BlockAabb {
                    min: cell_origin + cmin,
                    max: cell_origin + cmax,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Block lookup helpers
// ---------------------------------------------------------------------------

/// Looks up the block at world-space integer position `pos` from the chunk
/// cache.  Returns `Block::default()` (air) when the chunk is not loaded or
/// the Y coordinate is out of range.
fn get_block(pos: BlockPos, cache: &ChunkCache) -> Block {
    if pos.y < 0 || pos.y >= CHUNK_SIZE_Y as i32 {
        return Block::default();
    }
    let chunk_pos = pos.chunk_pos();
    let Some(chunk) = cache.get(&chunk_pos) else {
        return Block::default();
    };
    let local = pos.chunk_local();
    chunk
        .get(local.x as usize, local.y as usize, local.z as usize)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Swept-axis resolution
// ---------------------------------------------------------------------------

/// Sweeps the entity AABB along one axis from `current` to `target` and
/// returns the furthest position reachable without penetrating a solid block.
///
/// `velocity_component` is passed mutably so it can be zeroed on impact.
/// `grounded` is set when movement is stopped on the −Y face (landing).
///
/// # Parameters
///
/// - `current`  — entity origin before this frame's movement on `axis`.
/// - `target`   — desired entity origin after movement on `axis`.
/// - `aabb`     — entity's AABB.
/// - `axis`     — which world axis we are sweeping.
/// - `cache`    — read-only block data.
/// - `registry` — block registry for collision shape and solidity lookups.
/// - `velocity` — mutable reference to the entity velocity component.
/// - `grounded` — mutable reference to the grounded flag.
fn sweep_axis(
    current: Vec3,
    target: Vec3,
    aabb: &Aabb,
    axis: Axis,
    cache: &ChunkCache,
    registry: &BlockRegistry,
    velocity: &mut Velocity,
    grounded: &mut Grounded,
) -> Vec3 {
    let delta = match axis {
        Axis::X => target.x - current.x,
        Axis::Y => target.y - current.y,
        Axis::Z => target.z - current.z,
    };

    // Nothing to sweep.
    if delta.abs() < f32::EPSILON {
        return target;
    }

    let moving_positive = delta > 0.0;

    // Compute entity AABB corners at the *current* position (before the move
    // on this axis).  The cross-section test uses these.
    let e_min = aabb.min(current);
    let e_max = aabb.max(current);

    // The integer range of blocks the AABB sweeps through on the chosen axis.
    // We include a small epsilon so we do not miss blocks on exact boundaries.
    let (block_min, block_max) = swept_block_range(e_min, e_max, delta, axis);

    // Also compute the range of blocks the AABB covers on the *other two* axes
    // so we can skip cells that are not in the cross-section.
    let (cross_min_a, cross_max_a, cross_min_b, cross_max_b) =
        cross_section_ranges(e_min, e_max, axis);

    // Best (nearest) stop position along the sweep axis, initialised to the
    // full-displacement target.
    let mut resolved = match axis {
        Axis::X => target.x,
        Axis::Y => target.y,
        Axis::Z => target.z,
    };
    let mut hit = false;

    // Iterate the slab of blocks the entity sweeps through.
    'outer: for bx in cross_min_a..=cross_max_a {
        for bz in cross_min_b..=cross_max_b {
            for by in block_min..=block_max {
                // Map loop variables to world block coordinates depending on axis.
                let block_pos = match axis {
                    Axis::X => BlockPos::new(by, bx, bz),
                    Axis::Y => BlockPos::new(bx, by, bz),
                    Axis::Z => BlockPos::new(bx, bz, by),
                };

                let block = get_block(block_pos, cache);
                let Some(baabb) = block_world_aabb(block_pos, block, registry) else {
                    continue;
                };

                // Cross-section check: skip this block if the entity does not
                // overlap it on the two perpendicular axes.
                if !baabb.overlaps_cross_section(e_min, e_max, axis) {
                    continue;
                }

                // Compute the face of the block that the entity would hit.
                let (block_face, entity_face) = if moving_positive {
                    // Entity moving in + direction: entity's + face hits block's − face.
                    match axis {
                        Axis::X => (baabb.min.x, e_max.x),
                        Axis::Y => (baabb.min.y, e_max.y),
                        Axis::Z => (baabb.min.z, e_max.z),
                    }
                } else {
                    // Entity moving in − direction: entity's − face hits block's + face.
                    match axis {
                        Axis::X => (baabb.max.x, e_min.x),
                        Axis::Y => (baabb.max.y, e_min.y),
                        Axis::Z => (baabb.max.z, e_min.z),
                    }
                };

                // Gap between entity face and block face at the *current*
                // (pre-move) position.  If already overlapping (gap ≤ 0) on
                // this axis, we skip so we do not over-correct.
                let gap = if moving_positive {
                    block_face - entity_face
                } else {
                    entity_face - block_face
                };

                if gap < 0.0 {
                    // Already penetrating on this axis — skip to avoid
                    // over-correction.  A gap of exactly 0 means the entity
                    // is touching (but not inside) the block face; we still
                    // want to record this as a stop so that e.g. an entity
                    // standing exactly on a block top is recognised as grounded.
                    continue;
                }

                // Compute the position of the entity origin that places it
                // flush against this block face.
                let stop_component = match axis {
                    Axis::X => {
                        if moving_positive {
                            current.x + gap
                        } else {
                            current.x - gap
                        }
                    }
                    Axis::Y => {
                        if moving_positive {
                            current.y + gap
                        } else {
                            current.y - gap
                        }
                    }
                    Axis::Z => {
                        if moving_positive {
                            current.z + gap
                        } else {
                            current.z - gap
                        }
                    }
                };

                // Keep the nearest stop (smallest displacement).
                let is_nearer = if moving_positive {
                    stop_component < resolved
                } else {
                    stop_component > resolved
                };

                if is_nearer {
                    resolved = stop_component;
                    hit = true;
                }

                // Early-out: if the nearest-so-far stop is already at or
                // behind current, no further blocks can do worse.
                if (resolved
                    - match axis {
                        Axis::X => current.x,
                        Axis::Y => current.y,
                        Axis::Z => current.z,
                    })
                .abs()
                    < f32::EPSILON
                {
                    break 'outer;
                }
            }
        }
    }

    if hit {
        // Zero out the velocity component so the entity does not accumulate
        // speed through a wall.
        match axis {
            Axis::X => velocity.0.x = 0.0,
            Axis::Y => {
                // Landing: set grounded flag when stopped by a block below.
                if velocity.0.y < 0.0 {
                    grounded.0 = true;
                }
                velocity.0.y = 0.0;
            }
            Axis::Z => velocity.0.z = 0.0,
        }
    }

    // Reconstruct the full position with the resolved component.
    match axis {
        Axis::X => Vec3::new(resolved, target.y, target.z),
        Axis::Y => Vec3::new(target.x, resolved, target.z),
        Axis::Z => Vec3::new(target.x, target.y, resolved),
    }
}

/// Computes the inclusive integer block range swept by the entity's AABB
/// along `axis` when moving by `delta`.
fn swept_block_range(e_min: Vec3, e_max: Vec3, delta: f32, axis: Axis) -> (i32, i32) {
    let (face_behind, face_ahead) = match axis {
        Axis::X => (e_min.x, e_max.x),
        Axis::Y => (e_min.y, e_max.y),
        Axis::Z => (e_min.z, e_max.z),
    };

    // The entity already occupies [face_behind, face_ahead] on this axis.
    // After the move it will occupy [face_behind + delta, face_ahead + delta]
    // (before collision).  We scan all integer cells in the union of both.
    let (start, end) = if delta >= 0.0 {
        // Moving positive: the leading edge advances.
        (
            face_behind.floor() as i32,
            (face_ahead + delta).ceil() as i32 - 1,
        )
    } else {
        // Moving negative: the trailing edge retreats.
        (
            (face_behind + delta).floor() as i32,
            face_ahead.ceil() as i32 - 1,
        )
    };

    (start, end)
}

/// Returns the inclusive integer ranges of the two axes *perpendicular* to
/// `sweep_axis` that the entity AABB covers.
///
/// Return value: `(cross_a_min, cross_a_max, cross_b_min, cross_b_max)`
/// where `a` and `b` are the two perpendicular axes in the order
/// (X or Y, Z or Y) depending on `sweep_axis`:
///
/// | sweep_axis | a   | b   |
/// |------------|-----|-----|
/// | X          | Y   | Z   |
/// | Y          | X   | Z   |
/// | Z          | X   | Y   |
fn cross_section_ranges(e_min: Vec3, e_max: Vec3, axis: Axis) -> (i32, i32, i32, i32) {
    match axis {
        Axis::X => {
            let ya = e_min.y.floor() as i32;
            let yb = (e_max.y - f32::EPSILON).floor() as i32;
            let za = e_min.z.floor() as i32;
            let zb = (e_max.z - f32::EPSILON).floor() as i32;
            (ya, yb, za, zb)
        }
        Axis::Y => {
            let xa = e_min.x.floor() as i32;
            let xb = (e_max.x - f32::EPSILON).floor() as i32;
            let za = e_min.z.floor() as i32;
            let zb = (e_max.z - f32::EPSILON).floor() as i32;
            (xa, xb, za, zb)
        }
        Axis::Z => {
            let xa = e_min.x.floor() as i32;
            let xb = (e_max.x - f32::EPSILON).floor() as i32;
            let ya = e_min.y.floor() as i32;
            let yb = (e_max.y - f32::EPSILON).floor() as i32;
            (xa, xb, ya, yb)
        }
    }
}

// ---------------------------------------------------------------------------
// Main system
// ---------------------------------------------------------------------------

/// Resolves [`TentativePosition`] against the solid block grid.
///
/// Sweeps Y first (so [`Grounded`] is set before X/Z friction is applied),
/// then X, then Z.  Each axis is independent, which correctly handles walking
/// into a wall while falling without losing vertical accuracy.
///
/// Runs in [`PhysicsSet::BlockCollision`] during [`FixedUpdate`].
fn resolve_block_collisions(
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut query: Query<
        (
            &Transform,
            &Aabb,
            &mut TentativePosition,
            &mut Velocity,
            &mut Grounded,
        ),
        With<PhysicsBody>,
    >,
) {
    for (transform, aabb, mut tentative, mut velocity, mut grounded) in &mut query {
        let current = transform.translation;
        let target = tentative.0;

        // Sweep Y first so the grounded flag is available to X/Z friction
        // inside the same tick.
        let after_y = sweep_axis(
            current,
            Vec3::new(current.x, target.y, current.z),
            aabb,
            Axis::Y,
            &cache,
            &registry,
            &mut velocity,
            &mut grounded,
        );

        // Then sweep X with the Y-resolved position as the new base.
        let after_x = sweep_axis(
            Vec3::new(current.x, after_y.y, current.z),
            Vec3::new(target.x, after_y.y, current.z),
            aabb,
            Axis::X,
            &cache,
            &registry,
            &mut velocity,
            &mut grounded,
        );

        // Finally sweep Z.
        let after_z = sweep_axis(
            Vec3::new(after_x.x, after_y.y, current.z),
            Vec3::new(after_x.x, after_y.y, target.z),
            aabb,
            Axis::Z,
            &cache,
            &registry,
            &mut velocity,
            &mut grounded,
        );

        tentative.0 = Vec3::new(after_x.x, after_y.y, after_z.z);
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires the block-collision system into the Bevy schedule.
pub(super) struct BlockCollisionPlugin;

impl Plugin for BlockCollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            resolve_block_collisions.in_set(PhysicsSet::BlockCollision),
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::{BlockDefinition, BlockId},
        character::physics::{GravityScale, PhysicsBody, PhysicsPlugin},
        chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Z, Chunk, ChunkPos, cache::ChunkCache},
    };
    use bevy::time::TimeUpdateStrategy;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    /// Builds a minimal app with a pre-populated `ChunkCache`.
    ///
    /// Sets both the manual wall-clock duration **and** the `Time<Fixed>`
    /// timestep to `dt_secs` so that exactly one `FixedUpdate` tick fires per
    /// `tick()` call.
    fn make_app(dt_secs: f32) -> App {
        use bevy::time::Fixed;

        let duration = std::time::Duration::from_secs_f32(dt_secs);
        let mut app = App::new();
        app.add_plugins((bevy::MinimalPlugins, PhysicsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
            .insert_resource(BlockRegistry::new())
            .init_resource::<ChunkCache>();

        // Match the fixed timestep to the manual duration so the accumulator
        // overflows on every app.update() call after the seed frame.
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(duration);

        app
    }

    /// Runs `app.update()` enough times to guarantee that `FixedUpdate` has
    /// fired at least once.  With matching manual/fixed durations this is two
    /// frames: the first seeds the real-time clock with a non-zero delta, the
    /// second overflows the accumulator.
    fn tick(app: &mut App) {
        app.update(); // seed real-time clock
        app.update(); // overflow accumulator → FixedUpdate fires
    }

    /// Fills a flat floor at y = 0 in the chunk that contains (0, 0) with
    /// solid stone blocks (BlockId 1).
    fn fill_floor(app: &mut App, floor_y: i32) {
        // Register a solid block so solidity checks pass.
        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                chunk.set(lx, floor_y as usize, lz, Block::new(BlockId(1)));
            }
        }
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        cache.insert(chunk);
    }

    fn spawn_body(app: &mut App, origin: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(origin),
                PhysicsBody,
                Aabb::player(),
                GravityScale(0.0), // disable gravity so we control velocity manually
            ))
            .id()
    }

    // ------------------------------------------------------------------

    #[test]
    fn entity_does_not_fall_through_floor() {
        // Floor at y = 0.  Entity spawned at y = 1 (feet at y=1, just above floor).
        // Give it a large downward velocity; after collision it should sit on y=1.
        let mut app = make_app(1.0 / 20.0); // 50 ms tick
        fill_floor(&mut app, 0);

        // Spawn with feet at y = 2 so gravity can pull it down into the floor.
        let entity = spawn_body(&mut app, Vec3::new(0.5, 2.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -50.0; // large downward velocity
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // The floor top is at y=1.0, so the entity's feet (origin) should be
        // at or above y=1.0 after resolution.
        assert!(
            transform.translation.y >= 1.0 - 1e-3,
            "entity fell through floor: y = {}",
            transform.translation.y
        );
    }

    #[test]
    fn entity_grounded_when_on_floor() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        // Spawn with feet exactly at y=1 (sitting on the floor).
        let entity = spawn_body(&mut app, Vec3::new(0.5, 1.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -5.0; // small downward push to trigger landing
        }

        tick(&mut app);

        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            grounded.0,
            "entity should be grounded after landing on floor"
        );
    }

    #[test]
    fn entity_not_grounded_when_airborne() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        // Spawn high up with upward velocity — should not be grounded.
        let entity = spawn_body(&mut app, Vec3::new(0.5, 10.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = 5.0; // moving upward
        }

        tick(&mut app);

        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            !grounded.0,
            "entity should not be grounded while moving upward"
        );
    }

    #[test]
    fn entity_blocked_by_wall_x() {
        // Build a wall at x=2 (fills x=2 cells in the chunk).
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Wall: fill column x=2, all y, all z.
        for ly in 0..10usize {
            for lz in 0..CHUNK_SIZE_Z {
                chunk.set(2, ly, lz, Block::new(BlockId(1)));
            }
        }
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        // Entity at x=1.0 (feet at origin y=0) with large +X velocity.
        let entity = spawn_body(&mut app, Vec3::new(1.0, 0.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.x = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // Wall face is at x=2. Entity half-width is 0.3, so max-x = pos.x + 0.3.
        // After collision entity's x centre should be <= 2.0 - 0.3 = 1.7.
        assert!(
            transform.translation.x <= 1.7 + 1e-3,
            "entity should be stopped by wall at x=2, got x={}",
            transform.translation.x
        );

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.x.abs() < 1e-3,
            "X velocity should be zeroed on wall impact, got {}",
            vel.0.x
        );
    }

    #[test]
    fn block_world_aabb_full_cube() {
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_solid(true)
                .with_renderable(false),
            // collision_shape defaults to CollisionShape::FullCube
        );
        let pos = BlockPos::new(3, 5, 7);
        let block = Block::new(BlockId(1));

        let aabb =
            block_world_aabb(pos, block, &registry).expect("stone should have a collision AABB");

        assert!((aabb.min.x - 3.0).abs() < 1e-5);
        assert!((aabb.min.y - 5.0).abs() < 1e-5);
        assert!((aabb.min.z - 7.0).abs() < 1e-5);
        assert!((aabb.max.x - 4.0).abs() < 1e-5);
        assert!((aabb.max.y - 6.0).abs() < 1e-5);
        assert!((aabb.max.z - 8.0).abs() < 1e-5);
    }

    #[test]
    fn block_world_aabb_air_is_none() {
        let registry = BlockRegistry::new();
        let pos = BlockPos::new(0, 0, 0);
        let air = Block::new(BlockId::AIR);

        let aabb = block_world_aabb(pos, air, &registry);
        assert!(aabb.is_none(), "air should produce no collision AABB");
    }

    #[test]
    fn block_world_aabb_custom_shape_box() {
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(2), "slab")
                .with_solid(true)
                .with_renderable(false)
                .with_collision_shape(CollisionShape::Box {
                    min: Vec3::ZERO,
                    max: Vec3::new(1.0, 0.5, 1.0),
                }),
        );

        let pos = BlockPos::new(0, 0, 0);
        let block = Block::new(BlockId(2));
        let aabb = block_world_aabb(pos, block, &registry).expect("slab should have an AABB");

        assert!((aabb.max.y - 0.5).abs() < 1e-5, "slab top should be at 0.5");
    }

    #[test]
    fn block_world_aabb_none_shape_returns_none() {
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(3), "ghost")
                .with_solid(true) // solid but explicitly given no collision shape
                .with_renderable(false)
                .with_collision_shape(CollisionShape::None),
        );

        let pos = BlockPos::new(0, 0, 0);
        let block = Block::new(BlockId(3));
        let aabb = block_world_aabb(pos, block, &registry);
        assert!(
            aabb.is_none(),
            "CollisionShape::None should suppress collision"
        );
    }

    #[test]
    fn cross_section_ranges_x_axis() {
        // Entity AABB: x[-0.3, 0.3], y[0, 1.8], z[-0.3, 0.3]
        let e_min = Vec3::new(-0.3, 0.0, -0.3);
        let e_max = Vec3::new(0.3, 1.8, 0.3);
        let (ya, yb, za, zb) = cross_section_ranges(e_min, e_max, Axis::X);
        // Y floor(0)=0 .. floor(1.8 - eps)=1
        assert_eq!(ya, 0);
        assert_eq!(yb, 1);
        // Z floor(-0.3)=-1 .. floor(0.3 - eps)=0
        assert_eq!(za, -1);
        assert_eq!(zb, 0);
    }

    /// Verifies that an entity moving rapidly in the −Y direction into
    /// negative-Y space (no blocks present) neither panics nor is spuriously
    /// grounded.
    #[test]
    fn entity_above_negative_y_boundary_is_not_affected() {
        let mut app = make_app(1.0 / 20.0);
        // No floor — chunk cache is empty.

        // Spawn just above y=0 with a large downward velocity.
        let entity = spawn_body(&mut app, Vec3::new(0.5, 0.5, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.y < 0.5,
            "entity should have moved downward into negative-Y space, got y={}",
            transform.translation.y
        );

        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            !grounded.0,
            "entity should not be spuriously grounded with no blocks present"
        );
    }

    /// Verifies that an entity at the X chunk boundary (between chunk (0,0)
    /// and chunk (1,0)) is stopped by a floor that spans both chunks, i.e.
    /// chunk-boundary crossing during a sweep works correctly.
    #[test]
    fn entity_at_chunk_boundary_x_does_not_fall_through() {
        let mut app = make_app(1.0 / 20.0);

        // Register stone (only needs to happen once).
        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        // Fill floor at y=0 in chunk (0, 0) — local x 0..16.
        let mut chunk0 = Chunk::new(ChunkPos::new(0, 0));
        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                chunk0.set(lx, 0, lz, Block::new(BlockId(1)));
            }
        }

        // Fill floor at y=0 in chunk (1, 0) — world x 16..32, local x 0..16.
        let mut chunk1 = Chunk::new(ChunkPos::new(1, 0));
        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                chunk1.set(lx, 0, lz, Block::new(BlockId(1)));
            }
        }

        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk0);
            cache.insert(chunk1);
        }

        // Spawn exactly on the X chunk boundary with a large downward velocity.
        let entity = spawn_body(&mut app, Vec3::new(16.0, 2.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -50.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // Floor top is at y=1.0; entity feet (origin) should rest at or above it.
        assert!(
            transform.translation.y >= 1.0 - 1e-3,
            "entity fell through floor at chunk X boundary: y={}",
            transform.translation.y
        );
    }

    /// Verifies that diagonal movement into a concave corner (wall along X and
    /// wall along Z) does not clip through either surface.
    #[test]
    fn diagonal_movement_into_concave_corner_does_not_clip() {
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Wall along X: fill x=2, all z (0..16), y 0..5.
        for ly in 0..5usize {
            for lz in 0..CHUNK_SIZE_Z {
                chunk.set(2, ly, lz, Block::new(BlockId(1)));
            }
        }
        // Wall along Z: fill z=2, all x (0..16), y 0..5.
        for ly in 0..5usize {
            for lx in 0..CHUNK_SIZE_X {
                chunk.set(lx, ly, 2, Block::new(BlockId(1)));
            }
        }

        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        // Spawn in the open corner, drive diagonally toward the walls.
        let entity = spawn_body(&mut app, Vec3::new(1.0, 0.0, 1.0));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.x = 100.0;
            vel.0.z = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // Wall face at x=2; entity half-width 0.3 → max centre x = 1.7.
        assert!(
            transform.translation.x <= 1.7 + 1e-3,
            "entity clipped through X wall in concave corner: x={}",
            transform.translation.x
        );
        // Wall face at z=2; entity half-depth 0.3 → max centre z = 1.7.
        assert!(
            transform.translation.z <= 1.7 + 1e-3,
            "entity clipped through Z wall in concave corner: z={}",
            transform.translation.z
        );
    }

    /// Verifies that even an extreme downward velocity does not tunnel through
    /// a one-block-thick floor (anti-tunnelling / swept-collision guarantee).
    #[test]
    fn extreme_velocity_does_not_tunnel_through_thin_floor() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        // Start well above the floor with a velocity large enough to cross
        // many blocks in a single tick if unchecked.
        let entity = spawn_body(&mut app, Vec3::new(0.5, 10.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -10_000.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // Floor top is at y=1.0; entity should never go below it.
        assert!(
            transform.translation.y >= 1.0 - 1e-3,
            "extreme velocity tunnelled through floor: y={}",
            transform.translation.y
        );
    }

    /// Mirror of `entity_blocked_by_wall_x` but testing the Z axis: a wall
    /// at local z=2 should stop an entity moving in the +Z direction.
    #[test]
    fn entity_blocked_by_wall_z() {
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Wall: fill column z=2, all y 0..10, all x.
        for ly in 0..10usize {
            for lx in 0..CHUNK_SIZE_X {
                chunk.set(lx, ly, 2, Block::new(BlockId(1)));
            }
        }
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        // Entity at z=1.0 with large +Z velocity.
        let entity = spawn_body(&mut app, Vec3::new(0.5, 0.0, 1.0));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.z = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        // Wall face is at z=2. Entity half-depth is 0.3, so max-z = pos.z + 0.3.
        // After collision entity's z centre should be <= 2.0 - 0.3 = 1.7.
        assert!(
            transform.translation.z <= 1.7 + 1e-3,
            "entity should be stopped by wall at z=2, got z={}",
            transform.translation.z
        );

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.z.abs() < 1e-3,
            "Z velocity should be zeroed on wall impact, got {}",
            vel.0.z
        );
    }
}
