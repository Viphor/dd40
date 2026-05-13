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

use bevy::prelude::*;

use dd40_core::{
    block::registry::BlockRegistry,
    block::{Block, BlockPos, CollisionShape},
    chunk::cache::ChunkCache,
};
use dd40_physics_core::prelude::*;

use crate::integration::TentativePosition;

// ---------------------------------------------------------------------------
// Collision shape resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct BlockAabb {
    min: Vec3,
    max: Vec3,
}

impl BlockAabb {
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

fn block_world_aabb(pos: BlockPos, block: Block, registry: &BlockRegistry) -> Option<BlockAabb> {
    let shape = registry.collision_shape(&block);
    let cell_origin = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);

    match shape {
        CollisionShape::None => None,
        CollisionShape::FullCube => Some(BlockAabb {
            min: cell_origin,
            max: cell_origin + Vec3::ONE,
        }),
        CollisionShape::Box { min, max } => {
            let cmin = min.clamp(Vec3::ZERO, Vec3::ONE);
            let cmax = max.clamp(Vec3::ZERO, Vec3::ONE);
            if cmin.x >= cmax.x || cmin.y >= cmax.y || cmin.z >= cmax.z {
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

/// Looks up the block at `pos` in the chunk cache.
///
/// Returns the default (air) block when the chunk is not loaded. Does **not**
/// enforce a world-Y bound, because the chunk cache is the source of truth
/// for which chunks exist — once the world supports vertical chunking, all
/// `chunk_pos.y` values map through the cache the same way.
fn get_block(pos: BlockPos, cache: &ChunkCache) -> Block {
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

#[allow(clippy::too_many_arguments)]
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

    if delta.abs() < f32::EPSILON {
        return target;
    }

    let moving_positive = delta > 0.0;

    let e_min = aabb.min(current);
    let e_max = aabb.max(current);

    let (block_min, block_max) = swept_block_range(e_min, e_max, delta, axis);

    let (cross_min_a, cross_max_a, cross_min_b, cross_max_b) =
        cross_section_ranges(e_min, e_max, axis);

    let mut resolved = match axis {
        Axis::X => target.x,
        Axis::Y => target.y,
        Axis::Z => target.z,
    };
    let mut hit = false;

    'outer: for bx in cross_min_a..=cross_max_a {
        for bz in cross_min_b..=cross_max_b {
            for by in block_min..=block_max {
                let block_pos = match axis {
                    Axis::X => BlockPos::new(by, bx, bz),
                    Axis::Y => BlockPos::new(bx, by, bz),
                    Axis::Z => BlockPos::new(bx, bz, by),
                };

                let block = get_block(block_pos, cache);
                let Some(baabb) = block_world_aabb(block_pos, block, registry) else {
                    continue;
                };

                if !baabb.overlaps_cross_section(e_min, e_max, axis) {
                    continue;
                }

                trace!(
                    "block_collision: {:?} sweep — candidate block id={} at {:?} \
                     (world aabb {:.3?}..{:.3?}), entity {:.3?}..{:.3?}",
                    axis, block.block_id.0, block_pos, baabb.min, baabb.max, e_min, e_max,
                );

                let (block_face, entity_face) = if moving_positive {
                    match axis {
                        Axis::X => (baabb.min.x, e_max.x),
                        Axis::Y => (baabb.min.y, e_max.y),
                        Axis::Z => (baabb.min.z, e_max.z),
                    }
                } else {
                    match axis {
                        Axis::X => (baabb.max.x, e_min.x),
                        Axis::Y => (baabb.max.y, e_min.y),
                        Axis::Z => (baabb.max.z, e_min.z),
                    }
                };

                let gap = if moving_positive {
                    block_face - entity_face
                } else {
                    entity_face - block_face
                };

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

                let is_nearer = if moving_positive {
                    stop_component < resolved
                } else {
                    stop_component > resolved
                };

                if is_nearer {
                    resolved = stop_component;
                    hit = true;
                    trace!(
                        "block_collision: {:?} sweep — new nearest stop at {:.4} \
                         (block id={} at {:?}, gap={:.4}{})",
                        axis,
                        resolved,
                        block.block_id.0,
                        block_pos,
                        gap,
                        if gap < 0.0 { ", ejecting" } else { "" },
                    );
                }

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
        match axis {
            Axis::X => velocity.0.x = 0.0,
            Axis::Y => {
                if velocity.0.y < 0.0 {
                    grounded.0 = true;
                }
                velocity.0.y = 0.0;
            }
            Axis::Z => velocity.0.z = 0.0,
        }
    }

    match axis {
        Axis::X => Vec3::new(resolved, target.y, target.z),
        Axis::Y => Vec3::new(target.x, resolved, target.z),
        Axis::Z => Vec3::new(target.x, target.y, resolved),
    }
}

fn swept_block_range(e_min: Vec3, e_max: Vec3, delta: f32, axis: Axis) -> (i32, i32) {
    let (face_behind, face_ahead) = match axis {
        Axis::X => (e_min.x, e_max.x),
        Axis::Y => (e_min.y, e_max.y),
        Axis::Z => (e_min.z, e_max.z),
    };

    let (start, end) = if delta >= 0.0 {
        (
            face_behind.floor() as i32,
            (face_ahead + delta).ceil() as i32 - 1,
        )
    } else {
        (
            (face_behind + delta).floor() as i32,
            face_ahead.ceil() as i32 - 1,
        )
    };

    (start, end)
}

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
/// then X, then Z.
///
/// Runs in [`PhysicsSet::BlockCollision`] during [`FixedUpdate`].
fn resolve_block_collisions(
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut query: Query<
        (
            &CharacterPosition,
            &Aabb,
            &mut TentativePosition,
            &mut Velocity,
            &mut Grounded,
        ),
        With<PhysicsBody>,
    >,
) {
    for (char_pos, aabb, mut tentative, mut velocity, mut grounded) in &mut query {
        let current = char_pos.0;
        let target = tentative.0;

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
pub(crate) struct BlockCollisionPlugin;

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
    use crate::plugin::PhysicsPlugin;
    use bevy::time::TimeUpdateStrategy;
    use dd40_core::{
        block::{Block, BlockDefinition, BlockId},
        chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Z, Chunk, ChunkPos, cache::ChunkCache},
    };

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    fn make_app(dt_secs: f32) -> App {
        use bevy::time::Fixed;

        let duration = std::time::Duration::from_secs_f32(dt_secs);
        let mut app = App::new();
        app.add_plugins((bevy::MinimalPlugins, PhysicsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
            .insert_resource(BlockRegistry::new())
            .init_resource::<ChunkCache>();

        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(duration);

        app
    }

    fn tick(app: &mut App) {
        app.update();
        app.update();
    }

    fn fill_floor(app: &mut App, floor_y: i32) {
        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
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
                GravityScale(0.0),
            ))
            .id()
    }

    // ------------------------------------------------------------------

    #[test]
    fn entity_does_not_fall_through_floor() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        let entity = spawn_body(&mut app, Vec3::new(0.5, 2.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -50.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
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

        let entity = spawn_body(&mut app, Vec3::new(0.5, 1.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -5.0;
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

        let entity = spawn_body(&mut app, Vec3::new(0.5, 10.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = 5.0;
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
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for ly in 0..10usize {
            for lz in 0..CHUNK_SIZE_Z {
                chunk.set(2, ly, lz, Block::new(BlockId(1)));
            }
        }
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        let entity = spawn_body(&mut app, Vec3::new(1.0, 0.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.x = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
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
                .with_solid(true)
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
        let e_min = Vec3::new(-0.3, 0.0, -0.3);
        let e_max = Vec3::new(0.3, 1.8, 0.3);
        let (ya, yb, za, zb) = cross_section_ranges(e_min, e_max, Axis::X);
        assert_eq!(ya, 0);
        assert_eq!(yb, 1);
        assert_eq!(za, -1);
        assert_eq!(zb, 0);
    }

    #[test]
    fn entity_above_negative_y_boundary_is_not_affected() {
        let mut app = make_app(1.0 / 20.0);

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

    #[test]
    fn entity_at_chunk_boundary_x_does_not_fall_through() {
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk0 = Chunk::new(ChunkPos::new(0, 0, 0));
        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                chunk0.set(lx, 0, lz, Block::new(BlockId(1)));
            }
        }

        let mut chunk1 = Chunk::new(ChunkPos::new(1, 0, 0));
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

        let entity = spawn_body(&mut app, Vec3::new(16.0, 2.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -50.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.y >= 1.0 - 1e-3,
            "entity fell through floor at chunk X boundary: y={}",
            transform.translation.y
        );
    }

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

        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for ly in 0..5usize {
            for lz in 0..CHUNK_SIZE_Z {
                chunk.set(2, ly, lz, Block::new(BlockId(1)));
            }
        }
        for ly in 0..5usize {
            for lx in 0..CHUNK_SIZE_X {
                chunk.set(lx, ly, 2, Block::new(BlockId(1)));
            }
        }

        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        let entity = spawn_body(&mut app, Vec3::new(1.0, 0.0, 1.0));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.x = 100.0;
            vel.0.z = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.x <= 1.7 + 1e-3,
            "entity clipped through X wall in concave corner: x={}",
            transform.translation.x
        );
        assert!(
            transform.translation.z <= 1.7 + 1e-3,
            "entity clipped through Z wall in concave corner: z={}",
            transform.translation.z
        );
    }

    #[test]
    fn extreme_velocity_does_not_tunnel_through_thin_floor() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        let entity = spawn_body(&mut app, Vec3::new(0.5, 10.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -10_000.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.y >= 1.0 - 1e-3,
            "extreme velocity tunnelled through floor: y={}",
            transform.translation.y
        );
    }

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

        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for ly in 0..10usize {
            for lx in 0..CHUNK_SIZE_X {
                chunk.set(lx, ly, 2, Block::new(BlockId(1)));
            }
        }
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk);
        }

        let entity = spawn_body(&mut app, Vec3::new(0.5, 0.0, 1.0));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.z = 100.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
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

    #[test]
    fn entity_collides_with_block_in_non_zero_y_chunk() {
        // A floor block lives in chunk (0, 1, 0) at world y = CHUNK_SIZE_Y.
        // An entity dropping from above must land on it, proving that
        // get_block looks up chunks at non-zero ChunkPos.y correctly.
        use dd40_core::chunk::CHUNK_SIZE_Y;
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let mut chunk_above = Chunk::new(ChunkPos::new(0, 1, 0));
        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                chunk_above.set(lx, 0, lz, Block::new(BlockId(1)));
            }
        }
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk_above);
        }

        let floor_world_y = CHUNK_SIZE_Y as f32;
        let entity = spawn_body(&mut app, Vec3::new(0.5, floor_world_y + 2.0, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -50.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.y >= floor_world_y + 1.0 - 1e-3,
            "entity fell through floor at chunk Y boundary: y={} (expected >= {})",
            transform.translation.y,
            floor_world_y + 1.0,
        );

        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            grounded.0,
            "entity should be grounded on floor in non-zero-y chunk"
        );
    }

    #[test]
    fn entity_aabb_straddling_y_chunk_boundary_collides_with_block_in_lower_chunk() {
        // Entity is moving upward; a block sits in chunk (0, 0, 0) at the
        // top of that chunk (world y = CHUNK_SIZE_Y - 1). The entity's AABB
        // straddles the chunk-Y boundary so the swept collision test must
        // examine the lower chunk even though the entity's centre is in
        // the upper one.
        use dd40_core::chunk::CHUNK_SIZE_Y;
        let mut app = make_app(1.0 / 20.0);

        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        // Ceiling block sitting at the top cell of the lower chunk.
        let mut chunk_below = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk_below.set(0, CHUNK_SIZE_Y - 1, 0, Block::new(BlockId(1)));
        // Empty upper chunk so cache lookup succeeds for the entity's centre.
        let chunk_above = Chunk::new(ChunkPos::new(0, 1, 0));
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(chunk_below);
            cache.insert(chunk_above);
        }

        // Position the entity so its AABB feet are around y = CHUNK_SIZE_Y
        // (just above the ceiling block). Move it down into the block.
        let start_y = CHUNK_SIZE_Y as f32 + 0.1;
        let entity = spawn_body(&mut app, Vec3::new(0.5, start_y, 0.5));
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -10.0;
        }

        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.y >= CHUNK_SIZE_Y as f32 - 1e-3,
            "entity penetrated ceiling block across chunk Y boundary: y={}",
            transform.translation.y,
        );
    }
}
