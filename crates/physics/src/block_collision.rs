//! Block-grid collision detection and resolution.
//!
//! # Algorithm
//!
//! Because every block occupies exactly one 1×1×1 unit cell we can resolve
//! collisions in **O(1) per axis** rather than building a broad-phase pipeline:
//!
//! 1. From the entity's `Aabb` and `TentativePosition` compute the set of
//!    integer block coordinates the AABB *could* overlap (a small bounding
//!    box in block-integer space, typically 2–4 cells per axis).
//! 2. For **each axis independently** (Y first so the grounded flag is correct,
//!    then X, then Z) sweep the AABB from its current position toward the
//!    tentative position, find the nearest blocking cell along that axis, and
//!    stop there.
//! 3. Look up each candidate block in [`ChunkCache`] in O(1) (hash-map
//!    lookup by `ChunkPos` then array-indexed local lookup).
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
// Stuck-body fallback
// ---------------------------------------------------------------------------

/// Maximum Manhattan distance (in cells) the unstuck fallback searches.
///
/// When a body's AABB still overlaps a solid cell after swept-axis
/// resolution — typically because a block was placed on top of it, or
/// it was spawned mid-block — the fallback walks outward in Manhattan
/// shells looking for an offset where the AABB is overlap-free. If
/// nothing fits within this radius, the body is left in place and a
/// warning is logged.
const MAX_UNSTUCK_RADIUS: i32 = 8;

/// Minimum penetration (per axis) required before considering a body
/// "stuck" inside a solid. The block-collision sweep snaps bodies flush
/// against walls, and floating-point rounding can leave a sub-micron
/// nominal overlap. Requiring at least 1 mm of penetration prevents the
/// unstuck fallback from spuriously teleporting a body that is merely
/// pressed against a wall (which would otherwise yank a player running
/// along a wall sideways by one cell).
const STUCK_PENETRATION_EPSILON: f32 = 1.0e-3;

/// True if the AABB anchored at `pos` overlaps any solid cell with at
/// least [`STUCK_PENETRATION_EPSILON`] of penetration on every axis.
fn aabb_overlaps_any_solid(
    pos: Vec3,
    aabb: &Aabb,
    cache: &ChunkCache,
    registry: &BlockRegistry,
) -> bool {
    let e_min = aabb.min(pos);
    let e_max = aabb.max(pos);
    let x0 = e_min.x.floor() as i32;
    let x1 = (e_max.x - f32::EPSILON).floor() as i32;
    let y0 = e_min.y.floor() as i32;
    let y1 = (e_max.y - f32::EPSILON).floor() as i32;
    let z0 = e_min.z.floor() as i32;
    let z1 = (e_max.z - f32::EPSILON).floor() as i32;

    for bx in x0..=x1 {
        for by in y0..=y1 {
            for bz in z0..=z1 {
                let bp = BlockPos::new(bx, by, bz);
                let block = get_block(bp, cache);
                let Some(baabb) = block_world_aabb(bp, block, registry) else {
                    continue;
                };
                if e_min.x + STUCK_PENETRATION_EPSILON < baabb.max.x
                    && e_max.x > baabb.min.x + STUCK_PENETRATION_EPSILON
                    && e_min.y + STUCK_PENETRATION_EPSILON < baabb.max.y
                    && e_max.y > baabb.min.y + STUCK_PENETRATION_EPSILON
                    && e_min.z + STUCK_PENETRATION_EPSILON < baabb.max.z
                    && e_max.z > baabb.min.z + STUCK_PENETRATION_EPSILON
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Searches Manhattan shells outward from `current` for an integer
/// cell-offset that yields a non-overlapping AABB position.
///
/// Within a shell, offsets are tried in this order:
/// 1. Least absolute vertical displacement (`|dy|`).
/// 2. Then prefer **upward** (positive `dy`) — matches the player
///    expectation that a body squashed by a placed block pops up,
///    not into the floor.
/// 3. Then lexicographic (`dx`, `dz`).
///
/// Returns the displaced position if a free spot is found within
/// [`MAX_UNSTUCK_RADIUS`], else `None`.
fn nearest_empty_position(
    current: Vec3,
    aabb: &Aabb,
    cache: &ChunkCache,
    registry: &BlockRegistry,
) -> Option<Vec3> {
    let mut shell: Vec<(i32, i32, i32)> = Vec::new();
    for r in 1..=MAX_UNSTUCK_RADIUS {
        shell.clear();
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    if dx.abs() + dy.abs() + dz.abs() == r {
                        shell.push((dx, dy, dz));
                    }
                }
            }
        }
        shell.sort_by_key(|&(dx, dy, dz)| (dy.abs(), -dy, dx, dz));
        for &(dx, dy, dz) in &shell {
            let candidate = current + Vec3::new(dx as f32, dy as f32, dz as f32);
            if !aabb_overlaps_any_solid(candidate, aabb, cache, registry) {
                return Some(candidate);
            }
        }
    }
    None
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
    let local = pos.to_local();
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
            &PhysicsPosition,
            &Aabb,
            &mut TentativePosition,
            &mut Velocity,
            &mut Grounded,
        ),
        With<PhysicsBody>,
    >,
) {
    query
        .par_iter_mut()
        .for_each(|(char_pos, aabb, mut tentative, mut velocity, mut grounded)| {
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

            let mut resolved = Vec3::new(after_x.x, after_y.y, after_z.z);

            if aabb_overlaps_any_solid(resolved, aabb, &cache, &registry) {
                match nearest_empty_position(resolved, aabb, &cache, &registry) {
                    Some(snapped) => {
                        trace!(
                            "block_collision: body stuck inside solid at {:.3?} — snapping to nearest empty cell at {:.3?}",
                            resolved, snapped,
                        );
                        resolved = snapped;
                        velocity.0 = Vec3::ZERO;
                        grounded.0 = false;
                    }
                    None => {
                        warn!(
                            "block_collision: body at {:.3?} is fully encased within {} cells — leaving in place",
                            resolved, MAX_UNSTUCK_RADIUS,
                        );
                    }
                }
            }

            tentative.0 = resolved;
        });
}

// ---------------------------------------------------------------------------
// Contact detection (post-resolution)
// ---------------------------------------------------------------------------

/// Maximum distance between a body's face and a block's opposing face
/// for the pair to count as "in contact".
///
/// Matches the bias used by the sweep code so a freshly snapped body
/// reliably reports contact on the next pass.
const CONTACT_EPSILON: f32 = 1.0e-3;

/// Scans each body's six AABB faces for adjacent solid block faces and
/// writes a [`BodyBlockContact`] for every hit.
///
/// Runs after [`resolve_block_collisions`] in
/// [`PhysicsSet::BlockCollision`] so it sees the post-resolution
/// position (including any unstuck snap).
fn detect_block_contacts(
    cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
    mut contacts: MessageWriter<BodyBlockContact>,
    mut scratch: Local<bevy::utils::Parallel<Vec<BodyBlockContact>>>,
    query: Query<(Entity, &Aabb, &TentativePosition), With<PhysicsBody>>,
) {
    query.par_iter().for_each(|(entity, aabb, tentative)| {
        scratch.scope(|out| {
            collect_block_contacts_for_body(entity, aabb, tentative.0, &cache, &registry, out);
        });
    });

    for c in scratch.drain() {
        contacts.write(c);
    }
}

/// Scans the six faces of one body's AABB and pushes a contact into
/// `out` for every adjacent solid-block face within
/// [`CONTACT_EPSILON`].  Pulled out so the parallel scan body stays
/// flat and so the helper is unit-testable.
fn collect_block_contacts_for_body(
    entity: Entity,
    aabb: &Aabb,
    pos: Vec3,
    cache: &ChunkCache,
    registry: &BlockRegistry,
    out: &mut Vec<BodyBlockContact>,
) {
    let e_min = aabb.min(pos);
    let e_max = aabb.max(pos);

    let bx0 = e_min.x.floor() as i32;
    let bx1 = (e_max.x - f32::EPSILON).floor() as i32;
    let by0 = e_min.y.floor() as i32;
    let by1 = (e_max.y - f32::EPSILON).floor() as i32;
    let bz0 = e_min.z.floor() as i32;
    let bz1 = (e_max.z - f32::EPSILON).floor() as i32;

    // ── -Y face (body's bottom) ─────────────────────────────────────
    let by_below = (e_min.y - CONTACT_EPSILON).floor() as i32;
    for bx in bx0..=bx1 {
        for bz in bz0..=bz1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx, by_below, bz), cache, registry)
                && (e_min.y - baabb.max.y).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::Y,
                    penetration: (baabb.max.y - e_min.y).max(0.0),
                });
            }
        }
    }

    // ── +Y face (body's top) ────────────────────────────────────────
    let by_above = (e_max.y + CONTACT_EPSILON).floor() as i32;
    for bx in bx0..=bx1 {
        for bz in bz0..=bz1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx, by_above, bz), cache, registry)
                && (baabb.min.y - e_max.y).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::NEG_Y,
                    penetration: (e_max.y - baabb.min.y).max(0.0),
                });
            }
        }
    }

    // ── -X face ─────────────────────────────────────────────────────
    let bx_west = (e_min.x - CONTACT_EPSILON).floor() as i32;
    for by in by0..=by1 {
        for bz in bz0..=bz1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx_west, by, bz), cache, registry)
                && (e_min.x - baabb.max.x).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::X,
                    penetration: (baabb.max.x - e_min.x).max(0.0),
                });
            }
        }
    }

    // ── +X face ─────────────────────────────────────────────────────
    let bx_east = (e_max.x + CONTACT_EPSILON).floor() as i32;
    for by in by0..=by1 {
        for bz in bz0..=bz1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx_east, by, bz), cache, registry)
                && (baabb.min.x - e_max.x).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::NEG_X,
                    penetration: (e_max.x - baabb.min.x).max(0.0),
                });
            }
        }
    }

    // ── -Z face ─────────────────────────────────────────────────────
    let bz_north = (e_min.z - CONTACT_EPSILON).floor() as i32;
    for bx in bx0..=bx1 {
        for by in by0..=by1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx, by, bz_north), cache, registry)
                && (e_min.z - baabb.max.z).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::Z,
                    penetration: (baabb.max.z - e_min.z).max(0.0),
                });
            }
        }
    }

    // ── +Z face ─────────────────────────────────────────────────────
    let bz_south = (e_max.z + CONTACT_EPSILON).floor() as i32;
    for bx in bx0..=bx1 {
        for by in by0..=by1 {
            if let Some((bp, baabb)) =
                solid_block_at(BlockPos::new(bx, by, bz_south), cache, registry)
                && (baabb.min.z - e_max.z).abs() < CONTACT_EPSILON
            {
                out.push(BodyBlockContact {
                    body: entity,
                    block: bp,
                    normal: Vec3::NEG_Z,
                    penetration: (e_max.z - baabb.min.z).max(0.0),
                });
            }
        }
    }
}

/// Returns the block + its world-space AABB if there is a solid (or
/// shaped) collider at `bp`, else `None`.
fn solid_block_at(
    bp: BlockPos,
    cache: &ChunkCache,
    registry: &BlockRegistry,
) -> Option<(BlockPos, BlockAabb)> {
    let block = get_block(bp, cache);
    let baabb = block_world_aabb(bp, block, registry)?;
    Some((bp, baabb))
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
            (resolve_block_collisions, detect_block_contacts)
                .chain()
                .in_set(PhysicsSet::BlockCollision),
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

    // ------------------------------------------------------------------
    // Nearest-empty-cell unstuck fallback
    // ------------------------------------------------------------------

    /// Spawn a small (0.5-cube) physics body — sized so that any single
    /// empty cell can host it.  Used by the unstuck tests.
    fn spawn_small_body(app: &mut App, origin: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(origin),
                PhysicsBody,
                Aabb::new(0.25, 0.25, 0.25),
                GravityScale(0.0),
            ))
            .id()
    }

    /// Register a default solid block kind in the registry.
    fn register_stone(app: &mut App) {
        let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
        registry.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_solid(true)
                .with_renderable(false),
        );
    }

    /// Build a chunk at the origin with `set_cells` filled with stone.
    fn insert_chunk_with_cells(app: &mut App, set_cells: impl Fn(&mut Chunk)) {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        set_cells(&mut chunk);
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        cache.insert(chunk);
    }

    #[test]
    fn body_stuck_in_block_snaps_to_nearest_empty() {
        // Single solid block at (5,5,5); body has zero velocity (so the
        // swept-axis pass cannot eject it) and is centred inside the
        // block.  Unstuck fires and snaps to a Manhattan-distance-1
        // empty neighbour.  Specifically: tie-break `(|dy|, -dy, dx, dz)`
        // prefers least vertical displacement, then lex(dx, dz), so the
        // chosen neighbour is `dx = -1` → (4.5, 5.5, 5.5).
        let mut app = make_app(1.0 / 20.0);
        register_stone(&mut app);
        insert_chunk_with_cells(&mut app, |c| {
            c.set(5, 5, 5, Block::new(BlockId(1)));
        });

        let entity = spawn_small_body(&mut app, Vec3::new(5.5, 5.5, 5.5));
        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            (transform.translation.x - 4.5).abs() < 1e-3
                && (transform.translation.y - 5.5).abs() < 1e-3
                && (transform.translation.z - 5.5).abs() < 1e-3,
            "expected snap to (-x) neighbour at (4.5, 5.5, 5.5), got {:?}",
            transform.translation,
        );

        // Sanity: the resulting position is genuinely empty.
        assert!(
            !aabb_overlaps_any_solid(
                transform.translation,
                &Aabb::new(0.25, 0.25, 0.25),
                app.world().resource::<ChunkCache>(),
                app.world().resource::<BlockRegistry>(),
            ),
            "post-snap position must not overlap any solid",
        );
    }

    #[test]
    fn unstuck_clears_grounded_flag() {
        // Body stationary inside a solid block; pre-set Grounded so we
        // can observe it being cleared by the unstuck branch.
        let mut app = make_app(1.0 / 20.0);
        register_stone(&mut app);
        insert_chunk_with_cells(&mut app, |c| {
            c.set(5, 5, 5, Block::new(BlockId(1)));
        });

        let entity = spawn_small_body(&mut app, Vec3::new(5.5, 5.5, 5.5));
        {
            let mut grounded = app.world_mut().get_mut::<Grounded>(entity).unwrap();
            grounded.0 = true;
        }

        tick(&mut app);

        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            !grounded.0,
            "Grounded must be cleared on unstuck — next tick re-establishes it",
        );
    }

    #[test]
    fn unstuck_prefers_side_when_directly_above_is_blocked() {
        // Body inside (5,5,5); +y also blocked.  With tie-break
        // (|dy|, -dy, dx, dz) the radius-1 candidates with dy=0 are
        // tried before any dy=-1 candidate, and within dy=0 the
        // smallest dx wins → snap to (4.5, 5.5, 5.5).
        let mut app = make_app(1.0 / 20.0);
        register_stone(&mut app);
        insert_chunk_with_cells(&mut app, |c| {
            c.set(5, 5, 5, Block::new(BlockId(1)));
            c.set(5, 6, 5, Block::new(BlockId(1)));
        });

        let entity = spawn_small_body(&mut app, Vec3::new(5.5, 5.5, 5.5));
        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            (transform.translation.x - 4.5).abs() < 1e-3
                && (transform.translation.y - 5.5).abs() < 1e-3
                && (transform.translation.z - 5.5).abs() < 1e-3,
            "expected snap to (-x) neighbour at (4.5, 5.5, 5.5), got {:?}",
            transform.translation,
        );
    }

    #[test]
    fn body_not_inside_solid_is_not_displaced() {
        let mut app = make_app(1.0 / 20.0);
        register_stone(&mut app);
        insert_chunk_with_cells(&mut app, |c| {
            c.set(5, 5, 5, Block::new(BlockId(1)));
        });

        // Body in the air cell next to the solid; unstuck must not fire.
        let entity = spawn_small_body(&mut app, Vec3::new(7.5, 5.5, 5.5));
        tick(&mut app);

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            (transform.translation.x - 7.5).abs() < 1e-3
                && (transform.translation.y - 5.5).abs() < 1e-3
                && (transform.translation.z - 5.5).abs() < 1e-3,
            "free-air body must not be displaced, got {:?}",
            transform.translation,
        );
    }

    #[test]
    fn block_placed_on_resting_body_snaps_it_to_neighbour() {
        // Body resting on the floor at cell (5,1,5); next tick a
        // block is placed at (5,1,5) on top of the body.  The
        // collision pass must move the body to an empty neighbour.
        let mut app = make_app(1.0 / 20.0);
        register_stone(&mut app);
        insert_chunk_with_cells(&mut app, |c| {
            c.set(5, 0, 5, Block::new(BlockId(1)));
        });

        let entity = spawn_small_body(&mut app, Vec3::new(5.5, 1.5, 5.5));
        tick(&mut app);
        // Body should now be resting in the cell above the floor.
        let before = app.world().get::<Transform>(entity).unwrap().translation;
        assert!(
            (before.x - 5.5).abs() < 1e-3 && (before.z - 5.5).abs() < 1e-3,
            "body should be stable at x=5.5, z=5.5 before block placement, got {:?}",
            before,
        );

        // Place a block exactly where the body's centre is.
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            let mut chunk = cache.get(&ChunkPos::new(0, 0, 0)).unwrap().clone();
            chunk.set(5, before.y.floor() as usize, 5, Block::new(BlockId(1)));
            cache.insert(chunk);
        }
        tick(&mut app);

        let after = app.world().get::<Transform>(entity).unwrap().translation;
        assert!(
            !aabb_overlaps_any_solid(
                after,
                &Aabb::new(0.25, 0.25, 0.25),
                app.world().resource::<ChunkCache>(),
                app.world().resource::<BlockRegistry>(),
            ),
            "after unstuck the body's AABB must not overlap any solid, got {:?}",
            after,
        );
    }

    #[test]
    fn aabb_overlaps_any_solid_detects_centre_inside_block() {
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_solid(true)
                .with_renderable(false),
        );
        let mut cache = ChunkCache::default();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set(5, 5, 5, Block::new(BlockId(1)));
        cache.insert(chunk);

        let aabb = Aabb::new(0.25, 0.25, 0.25);
        assert!(aabb_overlaps_any_solid(
            Vec3::new(5.5, 5.5, 5.5),
            &aabb,
            &cache,
            &registry,
        ));
        assert!(!aabb_overlaps_any_solid(
            Vec3::new(7.5, 5.5, 5.5),
            &aabb,
            &cache,
            &registry,
        ));
    }

    #[test]
    fn flush_against_wall_is_not_considered_stuck() {
        // Regression: a body resting flush against a wall (or with
        // sub-epsilon floating-point overlap from the collision sweep)
        // must NOT trigger the unstuck fallback — otherwise a player
        // running along a wall gets teleported one cell sideways.
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(BlockId(1), "stone")
                .with_solid(true)
                .with_renderable(false),
        );
        let mut cache = ChunkCache::default();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        // Wall along x = 6 (block [6,7] is solid).
        for y in 0..8 {
            for z in 0..8 {
                chunk.set(6, y, z, Block::new(BlockId(1)));
            }
        }
        cache.insert(chunk);

        let aabb = Aabb::player();
        // Player flush against wall: e_max.x exactly at 6.0.
        let flush_x = 6.0 - aabb.half_x;
        assert!(!aabb_overlaps_any_solid(
            Vec3::new(flush_x, 1.0, 3.5),
            &aabb,
            &cache,
            &registry,
        ));
        // Floating-point rounding: e_max.x = 6.0 + 1e-6 (≈ 1 micron of
        // penetration). Must still not count as stuck.
        let nearly_flush_x = flush_x + 1.0e-6;
        assert!(!aabb_overlaps_any_solid(
            Vec3::new(nearly_flush_x, 1.0, 3.5),
            &aabb,
            &cache,
            &registry,
        ));
        // But 1 cm of penetration IS considered stuck.
        let real_overlap_x = flush_x + 1.0e-2;
        assert!(aabb_overlaps_any_solid(
            Vec3::new(real_overlap_x, 1.0, 3.5),
            &aabb,
            &cache,
            &registry,
        ));
    }

    // ------------------------------------------------------------------
    // Contact messages
    // ------------------------------------------------------------------

    fn collect_block_contacts(app: &App) -> Vec<BodyBlockContact> {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<BodyBlockContact>>();
        messages.iter_current_update_messages().cloned().collect()
    }

    #[test]
    fn resting_body_emits_block_contact_every_tick_with_upward_normal() {
        let mut app = make_app(1.0 / 20.0);
        fill_floor(&mut app, 0);

        let entity = spawn_body(&mut app, Vec3::new(0.5, 1.0, 0.5));
        // Settle.
        tick(&mut app);
        tick(&mut app);

        let contacts = collect_block_contacts(&app);
        assert!(
            contacts
                .iter()
                .any(|c| c.body == entity && c.normal == Vec3::Y && c.block.y == 0),
            "resting body should emit a BodyBlockContact with +Y normal each tick, got {contacts:?}"
        );
    }

    #[test]
    fn body_with_no_adjacent_blocks_emits_no_contacts() {
        let mut app = make_app(1.0 / 20.0);
        // Register stone but don't fill any blocks.
        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }

        let entity = spawn_body(&mut app, Vec3::new(0.5, 50.0, 0.5));
        // Kill gravity so the body doesn't fall and find nothing anyway.
        app.world_mut().get_mut::<GravityScale>(entity).unwrap().0 = 0.0;
        tick(&mut app);

        let contacts = collect_block_contacts(&app);
        assert!(
            !contacts.iter().any(|c| c.body == entity),
            "free-floating body should emit no contacts, got {contacts:?}"
        );
    }

    #[test]
    fn body_against_wall_emits_horizontal_contact() {
        let mut app = make_app(1.0 / 20.0);

        // Register stone + build a wall along x = 3 (block [3,4]).
        {
            let mut registry = app.world_mut().resource_mut::<BlockRegistry>();
            registry.register_without_event(
                BlockDefinition::new(BlockId(1), "stone")
                    .with_solid(true)
                    .with_renderable(false),
            );
        }
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for y in 0..4 {
            for z in 0..4 {
                chunk.set(3, y, z, Block::new(BlockId(1)));
            }
        }
        // Floor.
        for x in 0..CHUNK_SIZE_X {
            for z in 0..CHUNK_SIZE_Z {
                chunk.set(x, 0, z, Block::new(BlockId(1)));
            }
        }
        app.world_mut().resource_mut::<ChunkCache>().insert(chunk);

        let entity = spawn_body(&mut app, Vec3::new(2.5, 1.0, 1.5));
        // Push the body into the wall.
        app.world_mut().get_mut::<Velocity>(entity).unwrap().0.x = 5.0;
        for _ in 0..10 {
            tick(&mut app);
        }

        let contacts = collect_block_contacts(&app);
        assert!(
            contacts
                .iter()
                .any(|c| c.body == entity && c.normal == Vec3::NEG_X && c.block.x == 3),
            "body pressed against wall on its +X face should emit a -X-normal contact, got {contacts:?}"
        );
    }
}
