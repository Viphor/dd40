//! Character-vs-character collision resolution.
//!
//! This module owns [`PhysicsSet::CharacterCollision`]: the third stage of
//! each physics tick.  It pushes apart any two [`CharacterCollider`] entities
//! whose [`Aabb`]s overlap in their [`TentativePosition`]s.
//!
//! # Algorithm
//!
//! Each tick, two systems run in sequence inside [`PhysicsSet::CharacterCollision`]:
//!
//! 1. **[`crate::spatial_cache::update_character_spatial_cache`]** — rebuilds the
//!    [`CharacterSpatialCache`] from the current [`TentativePosition`]s.
//!    This is O(n) in the number of characters.
//!
//! 2. **[`resolve_character_collisions`]** — iterates only the *candidate
//!    pairs* emitted by [`CharacterSpatialCache::candidate_pairs`]: pairs that
//!    share at least one chunk.  Characters in different chunks are culled
//!    entirely.  For each candidate pair it calls [`Aabb::penetration`] and,
//!    if an overlap is found, splits the horizontal correction between the two
//!    entities.
//!
//! # Vertical axis
//!
//! Only horizontal axes (X and Z) are adjusted.  Vertical separation is left
//! to the block-collision stage so characters do not launch each other into
//! the air.
//!
//! # Static characters
//!
//! A [`CharacterCollider`] entity without a [`Velocity`] component is treated
//! as an immovable obstacle: the dynamic entity absorbs the full correction.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use dd40_physics_core::prelude::*;

use crate::integration::TentativePosition;
use crate::spatial_cache::{CharacterSpatialCache, update_character_spatial_cache};

// ---------------------------------------------------------------------------
// Collision system
// ---------------------------------------------------------------------------

/// Resolves overlaps between candidate-pair [`CharacterCollider`] entities by
/// adjusting their [`TentativePosition`]s.
///
/// Only the X and Z components of the penetration vector are applied.
///
/// Runs in [`PhysicsSet::CharacterCollision`] during [`FixedUpdate`].
fn resolve_character_collisions(
    cache: Res<CharacterSpatialCache>,
    mut query: Query<
        (&Aabb, &mut TentativePosition, Option<&mut Velocity>),
        (With<CharacterCollider>, With<PhysicsBody>),
    >,
) {
    // ── Pass 1: accumulate corrections ───────────────────────────────────
    let mut corrections: HashMap<Entity, Vec3> = HashMap::new();

    for (entity_a, entity_b) in cache.candidate_pairs() {
        let Ok((aabb_a, tentative_a, vel_a)) = query.get(entity_a) else {
            continue;
        };
        let Ok((aabb_b, tentative_b, vel_b)) = query.get(entity_b) else {
            continue;
        };

        let Some(pen) = aabb_a.penetration(tentative_a.0, aabb_b, tentative_b.0) else {
            continue;
        };

        let horizontal = Vec3::new(pen.x, 0.0, pen.z);
        if horizontal.length_squared() < f32::EPSILON {
            continue;
        }

        const SEPARATION_BIAS: f32 = 1e-3;
        let biased = horizontal + horizontal.normalize_or_zero() * SEPARATION_BIAS;

        let a_dynamic = vel_a.is_some();
        let b_dynamic = vel_b.is_some();

        match (a_dynamic, b_dynamic) {
            (true, true) => {
                *corrections.entry(entity_a).or_insert(Vec3::ZERO) += biased * 0.5;
                *corrections.entry(entity_b).or_insert(Vec3::ZERO) -= biased * 0.5;
            }
            (true, false) => {
                *corrections.entry(entity_a).or_insert(Vec3::ZERO) += biased;
            }
            (false, true) => {
                *corrections.entry(entity_b).or_insert(Vec3::ZERO) -= biased;
            }
            (false, false) => {}
        }
    }

    // ── Pass 2: apply corrections ─────────────────────────────────────────
    for (entity, correction) in corrections {
        if correction.length_squared() < f32::EPSILON {
            continue;
        }

        let Ok((_, mut tentative, maybe_vel)) = query.get_mut(entity) else {
            continue;
        };

        tentative.0 += correction;

        if let Some(mut vel) = maybe_vel {
            if correction.x.abs() > f32::EPSILON {
                vel.0.x = 0.0;
            }
            if correction.z.abs() > f32::EPSILON {
                vel.0.z = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires the spatial-cache update and collision-resolution systems into the
/// Bevy schedule, both inside [`PhysicsSet::CharacterCollision`].
pub(crate) struct CharacterCollisionPlugin;

impl Plugin for CharacterCollisionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterSpatialCache>()
            .add_systems(
                FixedUpdate,
                (update_character_spatial_cache, resolve_character_collisions)
                    .chain()
                    .in_set(PhysicsSet::CharacterCollision),
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
    use dd40_core::block::BlockRegistry;
    use dd40_core::chunk::cache::ChunkCache;
    use bevy::time::{Fixed, TimeUpdateStrategy};

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn make_app(dt_secs: f32) -> App {
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

    fn spawn_character(app: &mut App, pos: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(pos),
                PhysicsBody,
                CharacterCollider,
                Aabb::player(),
                GravityScale(0.0),
            ))
            .id()
    }

    // ------------------------------------------------------------------
    // Basic collision behaviour
    // ------------------------------------------------------------------

    #[test]
    fn overlapping_characters_are_separated() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(0.2, 0.0, 0.0));

        tick(&mut app);

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;
        let aabb = Aabb::player();
        assert!(
            !aabb.overlaps(pos_a, &aabb, pos_b),
            "characters should not overlap: a={pos_a:?}, b={pos_b:?}"
        );
    }

    #[test]
    fn separated_characters_are_not_moved() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(10.0, 0.0, 0.0));

        tick(&mut app);

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;

        assert!(pos_a.x.abs() < 1e-3, "a should not have moved: {pos_a:?}");
        assert!((pos_b.x - 10.0).abs() < 1e-3, "b should not have moved: {pos_b:?}");
    }

    #[test]
    fn separation_is_symmetric_for_two_dynamic_bodies() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));

        tick(&mut app);

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;

        assert!(
            (pos_a.x - pos_b.x).abs() > f32::EPSILON
                || (pos_a.z - pos_b.z).abs() > f32::EPSILON,
            "same-position characters should be separated: a={pos_a:?}, b={pos_b:?}"
        );
        assert!(((pos_a.x + pos_b.x) * 0.5).abs() < 0.1, "midpoint X should stay near origin");
        assert!(((pos_a.z + pos_b.z) * 0.5).abs() < 0.1, "midpoint Z should stay near origin");
    }

    #[test]
    fn y_axis_not_affected_by_character_collision() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(0.0, 0.5, 0.0));

        tick(&mut app);

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;

        assert!((pos_a.y).abs() < 0.1, "Y of a should be unchanged: {}", pos_a.y);
        assert!((pos_b.y - 0.5).abs() < 0.1, "Y of b should be unchanged: {}", pos_b.y);
    }

    #[test]
    fn velocity_zeroed_on_separation_axis() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(0.2, 0.0, 0.0));

        app.world_mut().get_mut::<Velocity>(a).unwrap().0.x = 5.0;
        app.world_mut().get_mut::<Velocity>(b).unwrap().0.x = -5.0;

        tick(&mut app);

        let vel_a = app.world().get::<Velocity>(a).unwrap();
        let vel_b = app.world().get::<Velocity>(b).unwrap();
        assert!(vel_a.0.x.abs() < 1e-3, "a X vel should be zeroed, got {}", vel_a.0.x);
        assert!(vel_b.0.x.abs() < 1e-3, "b X vel should be zeroed, got {}", vel_b.0.x);
    }

    #[test]
    fn three_overlapping_characters_all_separated() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));
        let b = spawn_character(&mut app, Vec3::new(0.2, 0.0, 0.0));
        let c = spawn_character(&mut app, Vec3::new(0.4, 0.0, 0.0));

        for _ in 0..20 {
            tick(&mut app);
        }

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;
        let pos_c = app.world().get::<Transform>(c).unwrap().translation;
        let aabb = Aabb::player();

        assert!(!aabb.overlaps(pos_a, &aabb, pos_b), "a-b overlap: {pos_a:?} {pos_b:?}");
        assert!(!aabb.overlaps(pos_b, &aabb, pos_c), "b-c overlap: {pos_b:?} {pos_c:?}");
        assert!(!aabb.overlaps(pos_a, &aabb, pos_c), "a-c overlap: {pos_a:?} {pos_c:?}");
    }

    // ------------------------------------------------------------------
    // Chunk-boundary behaviour
    // ------------------------------------------------------------------

    #[test]
    fn characters_straddling_chunk_boundary_are_resolved() {
        let mut app = make_app(1.0 / 60.0);

        let a = spawn_character(&mut app, Vec3::new(16.0, 0.0, 4.0));
        let b = spawn_character(&mut app, Vec3::new(16.1, 0.0, 4.0));

        tick(&mut app);

        let pos_a = app.world().get::<Transform>(a).unwrap().translation;
        let pos_b = app.world().get::<Transform>(b).unwrap().translation;
        let aabb = Aabb::player();
        assert!(
            !aabb.overlaps(pos_a, &aabb, pos_b),
            "characters at chunk boundary should be separated: a={pos_a:?}, b={pos_b:?}"
        );
    }

    #[test]
    fn characters_in_different_chunks_not_paired() {
        let mut app = make_app(1.0 / 60.0);

        let e1 = spawn_character(&mut app, Vec3::new(4.0, 0.0, 4.0));
        let e2 = spawn_character(&mut app, Vec3::new(84.0, 0.0, 84.0));

        let start_e1 = Vec3::new(4.0, 0.0, 4.0);
        let start_e2 = Vec3::new(84.0, 0.0, 84.0);

        tick(&mut app);

        let pos_e1 = app.world().get::<Transform>(e1).unwrap().translation;
        let pos_e2 = app.world().get::<Transform>(e2).unwrap().translation;

        assert!(
            (pos_e1.x - start_e1.x).abs() < 1e-3,
            "e1 should not have been pushed: was {start_e1:?}, now {pos_e1:?}"
        );
        assert!(
            (pos_e2.x - start_e2.x).abs() < 1e-3,
            "e2 should not have been pushed: was {start_e2:?}, now {pos_e2:?}"
        );
    }

    #[test]
    fn character_moving_between_chunks_still_collides() {
        let mut app = make_app(1.0 / 20.0);

        let e1 = spawn_character(&mut app, Vec3::new(4.0, 0.0, 4.0));
        let e2 = spawn_character(&mut app, Vec3::new(20.0, 0.0, 4.0));

        tick(&mut app);
        {
            let cache = app.world().resource::<CharacterSpatialCache>();
            let pairs: Vec<_> = cache.candidate_pairs().collect();
            assert!(
                pairs.is_empty(),
                "no pair expected when characters are in different chunks: {pairs:?}"
            );
        }

        app.world_mut().get_mut::<Transform>(e1).unwrap().translation.x = 20.1;

        tick(&mut app);

        let pos_e1 = app.world().get::<Transform>(e1).unwrap().translation;
        let pos_e2 = app.world().get::<Transform>(e2).unwrap().translation;
        let aabb = Aabb::player();
        assert!(
            !aabb.overlaps(pos_e1, &aabb, pos_e2),
            "e1 in e2's chunk must collide: e1={pos_e1:?}, e2={pos_e2:?}"
        );
    }

    // ------------------------------------------------------------------
    // Static vs dynamic
    // ------------------------------------------------------------------

    #[test]
    fn static_obstacle_does_not_move() {
        let mut app = make_app(1.0 / 60.0);

        let obstacle_pos = Vec3::new(0.3, 0.0, 0.0);
        let obstacle = app
            .world_mut()
            .spawn((
                Transform::from_translation(obstacle_pos),
                PhysicsBody,
                CharacterCollider,
                Aabb::player(),
                GravityScale(0.0),
            ))
            .id();
        app.world_mut().entity_mut(obstacle).remove::<Velocity>();

        let dynamic = spawn_character(&mut app, Vec3::new(0.0, 0.0, 0.0));

        tick(&mut app);

        let obs_after = app.world().get::<Transform>(obstacle).unwrap().translation;
        let dyn_after = app.world().get::<Transform>(dynamic).unwrap().translation;

        assert!(
            (obs_after.x - obstacle_pos.x).abs() < 1e-3,
            "static obstacle must not move: was {obstacle_pos:?}, now {obs_after:?}"
        );
        let aabb = Aabb::player();
        assert!(
            !aabb.overlaps(dyn_after, &aabb, obs_after),
            "dynamic char must be pushed clear: dyn={dyn_after:?}, obs={obs_after:?}"
        );
    }
}
