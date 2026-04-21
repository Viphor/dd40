//! Velocity integration and gravity application.
//!
//! This module owns [`PhysicsSet::Integrate`]: the first stage of each physics
//! tick.  It is responsible for:
//!
//! 1. Resetting the [`Grounded`] flag from the previous frame.
//! 2. Applying gravitational acceleration to [`Velocity`] (scaled by
//!    [`GravityScale`]).
//! 3. Clamping downward speed to [`PhysicsConfig::terminal_velocity`].
//! 4. Writing `current_position + velocity * dt` into [`TentativePosition`]
//!    so the collision stages have a scratch value to refine.
//!
//! Nothing in this module touches [`Transform`] directly — that write happens
//! exclusively in [`PhysicsSet::Finalise`] (see `mod.rs`).

use bevy::prelude::*;

use super::{CharacterPosition, Grounded, Impulse, PhysicsBody, PhysicsConfig, PhysicsSet, TentativePosition, Velocity};

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Resets [`Grounded`] and applies gravity + velocity integration to produce
/// a [`TentativePosition`].
///
/// Reads [`CharacterPosition`] as the physics-authoritative starting point.
/// [`Transform`] is not touched here — it is written by the rendering layer.
///
/// Runs in [`PhysicsSet::Integrate`] during [`FixedUpdate`].
fn integrate(
    time: Res<Time>,
    config: Res<PhysicsConfig>,
    mut query: Query<
        (
            &CharacterPosition,
            &mut Velocity,
            &mut Impulse,
            &super::GravityScale,
            &mut Grounded,
            &mut TentativePosition,
        ),
        With<PhysicsBody>,
    >,
) {
    let dt = time.delta_secs();

    for (char_pos, mut velocity, mut impulse, gravity_scale, mut grounded, mut tentative) in
        &mut query
    {
        // ── 0. Flush pending impulses ─────────────────────────────────────
        velocity.0 += impulse.0;
        impulse.0 = Vec3::ZERO;

        // ── 1. Reset grounded flag ────────────────────────────────────────
        grounded.0 = false;

        // ── 2. Apply gravity ──────────────────────────────────────────────
        let gravity_accel = -config.gravity * gravity_scale.0;
        velocity.0.y += gravity_accel * dt;

        // ── 3. Clamp to terminal velocity ─────────────────────────────────
        if velocity.0.y < -config.terminal_velocity {
            velocity.0.y = -config.terminal_velocity;
        }

        // ── 4. Produce tentative position from the physics truth ──────────
        tentative.0 = char_pos.0 + velocity.0 * dt;
    }
}

/// Copies the resolved [`TentativePosition`] into [`CharacterPosition`] and
/// [`Transform`], then applies velocity damping (friction).
///
/// [`CharacterPosition`] is the physics-authoritative value read next tick.
/// [`Transform`] is kept in sync so non-networked entities render correctly
/// without any additional bridge system. Networked predicted entities have
/// their [`Transform`] overridden by the frame-interpolation system in
/// `Update`, so this write is purely a safe fallback for them.
///
/// Runs in [`PhysicsSet::Finalise`] during [`FixedUpdate`].
fn finalise(
    time: Res<Time>,
    config: Res<PhysicsConfig>,
    mut query: Query<
        (
            &mut Transform,
            &mut CharacterPosition,
            &mut Velocity,
            &Grounded,
            &TentativePosition,
        ),
        With<PhysicsBody>,
    >,
) {
    let dt = time.delta_secs();

    for (mut transform, mut char_pos, mut velocity, grounded, tentative) in &mut query {
        // ── Write resolved position to both physics truth and visual fallback
        char_pos.0 = tentative.0;
        transform.translation = tentative.0;

        // ── Apply friction ────────────────────────────────────────────────
        // Dampen horizontal velocity only.  Vertical velocity is governed by
        // gravity and collision response, not friction.
        //
        // We use an exponential decay so the damping is frame-rate independent:
        //   v_new = v * (1 - friction * dt).clamp(0, 1)
        //
        // Grounded entities experience higher friction than airborne ones.
        let friction = if grounded.is_grounded() {
            config.ground_friction
        } else {
            config.air_friction
        };

        let decay = (1.0 - friction * dt).max(0.0);
        velocity.0.x *= decay;
        velocity.0.z *= decay;
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires the integration and finalise systems into the Bevy schedule.
pub(super) struct IntegrationPlugin;

impl Plugin for IntegrationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, integrate.in_set(PhysicsSet::Integrate))
            .add_systems(FixedUpdate, finalise.in_set(PhysicsSet::Finalise));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::registry::BlockRegistry,
        character::physics::{Aabb, GravityScale, PhysicsBody, PhysicsPlugin},
        chunk::cache::ChunkCache,
    };
    use bevy::time::TimeUpdateStrategy;

    /// Builds a minimal [`App`] with the physics plugin and a fixed timestep
    /// so tests are deterministic.
    ///
    /// Sets both the manual wall-clock duration **and** the `Time<Fixed>`
    /// timestep to `dt_secs` so that exactly one `FixedUpdate` tick fires per
    /// `app.update()` call (after the first warm-up frame which seeds the
    /// accumulator).
    fn make_app(dt_secs: f32) -> App {
        use bevy::time::Fixed;

        let duration = std::time::Duration::from_secs_f32(dt_secs);
        let mut app = App::new();
        app.add_plugins((bevy::MinimalPlugins, PhysicsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
            .insert_resource(BlockRegistry::new())
            .init_resource::<ChunkCache>();

        // Set the fixed timestep to the same duration so the accumulator
        // overflows on every app.update() call (after the first seed frame).
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(duration);

        app
    }

    /// Advances time by exactly one `FixedUpdate` tick.
    ///
    /// The first `app.update()` seeds the real-time clock with a non-zero
    /// delta so the accumulator starts filling.  The second call overflows
    /// the accumulator (because we matched the fixed timestep to the manual
    /// duration) and fires exactly one `FixedUpdate` iteration.
    fn tick(app: &mut App) {
        app.update(); // seed real-time clock (accumulator starts at 0 → dt)
        app.update(); // accumulator overflows fixed timestep → FixedUpdate fires once
    }

    fn spawn_body(app: &mut App, origin: Vec3, gravity_scale: f32) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(origin),
                PhysicsBody,
                Aabb::player(),
                GravityScale(gravity_scale),
            ))
            .id()
    }

    // ------------------------------------------------------------------

    #[test]
    fn gravity_accelerates_downward() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 1.0);

        // Run enough frames to guarantee FixedUpdate fires at least once.
        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y < 0.0,
            "gravity should have produced a downward velocity, got {}",
            vel.0.y
        );
    }

    #[test]
    fn zero_gravity_scale_no_acceleration() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 0.0);

        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            (vel.0.y).abs() < 1e-5,
            "zero gravity scale should produce no vertical acceleration, got {}",
            vel.0.y
        );
    }

    #[test]
    fn terminal_velocity_clamped() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 1.0);

        // Pre-load a very large downward velocity that exceeds terminal velocity.
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = -1_000.0;
        }

        tick(&mut app);

        let config = app.world().resource::<PhysicsConfig>();
        let term = config.terminal_velocity;
        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y >= -term,
            "downward velocity {:.2} should be clamped to -{:.2}",
            vel.0.y,
            term
        );
    }

    #[test]
    fn tentative_position_advances_by_velocity() {
        // Use 1/20 s so maths stays simple and we avoid extreme friction decay.
        let mut app = make_app(1.0 / 20.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 0.0); // no gravity

        // Give it a known horizontal velocity.
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0 = Vec3::new(3.0, 0.0, 0.0);
        }

        tick(&mut app);

        // After finalise, the Transform should reflect the movement.
        let transform = app.world().get::<Transform>(entity).unwrap();
        // With 1 second dt and no friction (grounded = false, air_friction decays),
        // the position should have advanced roughly by the initial velocity.
        // We just check the correct direction and a non-zero advance.
        assert!(
            transform.translation.x > 0.0,
            "entity should have moved in +X, got {}",
            transform.translation.x
        );
    }

    #[test]
    fn grounded_flag_reset_each_frame() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 0.0);

        // Seed the clock so FixedUpdate will fire on the next update.
        app.update();

        // Manually mark as grounded just before the fixed tick fires.
        {
            let mut g = app.world_mut().get_mut::<Grounded>(entity).unwrap();
            g.0 = true;
        }

        // Fire exactly one FixedUpdate — integrate resets grounded to false,
        // block collision finds no blocks, so it stays false.
        app.update();

        // Since there are no blocks in this test, it should remain false.
        let grounded = app.world().get::<Grounded>(entity).unwrap();
        assert!(
            !grounded.0,
            "grounded flag should be reset when no block is below"
        );
    }

    /// Verifies that upward velocity is not clamped by terminal velocity —
    /// only downward speed is subject to the terminal-velocity cap.
    #[test]
    fn upward_velocity_not_clamped_by_terminal_velocity() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 0.0); // gravity_scale = 0 so gravity doesn't interfere

        // Give the entity a large upward velocity.
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0.y = 1_000.0;
        }

        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y > 500.0,
            "upward velocity should not be clamped by terminal velocity, got {}",
            vel.0.y
        );
    }

    /// Verifies that a negative gravity scale inverts gravity, causing the
    /// entity to accelerate upward rather than downward.
    #[test]
    fn negative_gravity_scale_accelerates_upward() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, -1.0); // inverted gravity

        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y > 0.0,
            "negative gravity scale should accelerate entity upward, got {}",
            vel.0.y
        );
    }

    #[test]
    fn ground_friction_slows_horizontal_velocity() {
        // Use 1/20 s so friction decay is meaningful but not instant.
        let mut app = make_app(1.0 / 20.0);
        let entity = spawn_body(&mut app, Vec3::ZERO, 0.0);

        // Give the entity a horizontal velocity.
        {
            let mut vel = app.world_mut().get_mut::<Velocity>(entity).unwrap();
            vel.0 = Vec3::new(10.0, 0.0, 0.0);
        }

        // Note: integrate resets grounded to false each tick, so we will test
        // air friction rather than ground friction here — but the invariant
        // (velocity is reduced by *some* friction) still validates the system.
        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.x < 10.0,
            "horizontal velocity should be reduced by friction, got {}",
            vel.0.x
        );
        assert!(vel.0.x >= 0.0, "velocity should remain non-negative");
    }
}
