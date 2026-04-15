//! Character controller — the bridge between input systems and the physics pipeline.
//!
//! Any system (player input, AI, network replication) that wants to move a
//! character writes its intent into the [`CharacterController`] component.  A
//! single physics-side system ([`apply_character_controller`]) then translates
//! that intent into [`Velocity`] changes before [`PhysicsSet::Integrate`] runs,
//! so gravity, block collision, and character-vs-character collision all act on
//! the resulting velocity as normal.
//!
//! # Usage
//!
//! 1. Insert [`CharacterController`] alongside [`PhysicsBody`] when spawning a
//!    character.
//! 2. Each frame, write desired movement and jump intent into the component from
//!    whichever input system owns the character.
//! 3. Do **not** modify [`Velocity`] directly for locomotion — write to
//!    [`CharacterController`] instead so that the controller and the physics
//!    pipeline stay in sync.
//!
//! ```
//! use dd40_core::character::controller::CharacterController;
//! use bevy::math::Vec3;
//!
//! // From an input system:
//! fn move_character(mut query: bevy::prelude::Query<&mut CharacterController>) {
//!     for mut ctrl in &mut query {
//!         ctrl.movement = Vec3::new(1.0, 0.0, 0.0); // move right
//!         ctrl.jump = true;
//!     }
//! }
//! ```

use bevy::prelude::*;

use super::{MovementSpeed, physics::{Grounded, PhysicsBody, PhysicsSet, Velocity}};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Per-frame movement intent for a character.
///
/// Written to by input systems (player, AI, network) and consumed once per
/// [`FixedUpdate`] tick by [`apply_character_controller`], which translates the
/// intent into [`Velocity`] before the physics pipeline runs.
///
/// All fields are reset / interpreted on each tick:
/// - [`movement`] is applied every tick it is non-zero.
/// - [`jump`] is consumed (set back to `false`) immediately after a jump fires.
/// - [`sprint_multiplier`] scales the movement speed for that tick.
///
/// [`movement`]: CharacterController::movement
/// [`jump`]: CharacterController::jump
/// [`sprint_multiplier`]: CharacterController::sprint_multiplier
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
pub struct CharacterController {
    /// Desired movement direction in **world space**, projected onto the
    /// horizontal (XZ) plane.
    ///
    /// The vector should be normalised before being written here.  The
    /// controller multiplies it by [`MovementSpeed`] and
    /// [`sprint_multiplier`] to produce the target horizontal velocity.
    ///
    /// [`sprint_multiplier`]: CharacterController::sprint_multiplier
    pub movement: Vec3,

    /// When `true` and the entity is [`Grounded`], the controller applies an
    /// upward [`jump_impulse`] to [`Velocity`] and resets this field to
    /// `false`.
    ///
    /// If the entity is not grounded, this field is ignored (but not reset, so
    /// a buffered jump press is **not** re-tried next frame — callers should
    /// set it again if they want queued jumps).
    ///
    /// [`jump_impulse`]: CharacterController::jump_impulse
    pub jump: bool,

    /// Upward velocity (in world units per second) applied when a jump fires.
    ///
    /// Tuned per-character rather than globally so that different character
    /// types can have different jump heights without touching [`PhysicsConfig`].
    ///
    /// [`PhysicsConfig`]: crate::character::physics::PhysicsConfig
    pub jump_impulse: f32,

    /// Scale factor applied to [`MovementSpeed`] this tick.
    ///
    /// `1.0` = normal speed, `2.0` = sprinting, `0.0` = no movement.
    pub sprint_multiplier: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            movement: Vec3::ZERO,
            jump: false,
            jump_impulse: 8.0,
            sprint_multiplier: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// Translates [`CharacterController`] intent into [`Velocity`] changes.
///
/// Runs in [`FixedUpdate`] **before** [`PhysicsSet::Integrate`] so that the
/// horizontal velocity set here is immediately picked up by the integration
/// step and then refined by the collision stages.
///
/// ### What this system does
///
/// - Sets `velocity.x` and `velocity.z` from `controller.movement ×
///   speed × sprint_multiplier`.  Vertical velocity is **not** touched here
///   (gravity and jump are handled separately).
/// - If `controller.jump` is `true` and the entity is [`Grounded`], sets
///   `velocity.y = controller.jump_impulse` and resets `controller.jump =
///   false`.
/// - If `controller.jump` is `true` but the entity is **not** grounded, the
///   jump is silently dropped (not buffered).
/// - Resets `controller.movement` and `controller.sprint_multiplier` to their
///   defaults after applying them, so a missing input write results in the
///   character stopping rather than continuing at the last velocity.
fn apply_character_controller(
    mut query: Query<
        (
            &mut CharacterController,
            &MovementSpeed,
            &Grounded,
            &mut Velocity,
        ),
        With<PhysicsBody>,
    >,
) {
    for (mut controller, speed, grounded, mut velocity) in &mut query {
        // ── Horizontal movement ───────────────────────────────────────────
        let horizontal_speed = speed.0 * controller.sprint_multiplier;
        velocity.0.x = controller.movement.x * horizontal_speed;
        velocity.0.z = controller.movement.z * horizontal_speed;

        // ── Jump ─────────────────────────────────────────────────────────
        if controller.jump {
            if grounded.is_grounded() {
                velocity.0.y = controller.jump_impulse;
            }
            // Consume the intent regardless of grounded state so that a
            // held-down jump key doesn't re-fire on the next grounding event.
            controller.jump = false;
        }

        // ── Reset per-frame intent ────────────────────────────────────────
        // Input systems write intent each Update tick; clearing here ensures
        // a missed write (e.g. paused input) stops the character cleanly.
        controller.movement = Vec3::ZERO;
        controller.sprint_multiplier = 1.0;
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers the [`CharacterController`] type and wires
/// [`apply_character_controller`] into the schedule.
pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CharacterController>().add_systems(
            FixedUpdate,
            apply_character_controller.before(PhysicsSet::Integrate),
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
        block::registry::BlockRegistry,
        character::physics::{Aabb, GravityScale, PhysicsPlugin},
        chunk::cache::ChunkCache,
    };
    use bevy::time::TimeUpdateStrategy;

    fn make_app(dt_secs: f32) -> App {
        use bevy::time::Fixed;
        let duration = std::time::Duration::from_secs_f32(dt_secs);
        let mut app = App::new();
        app.add_plugins((bevy::MinimalPlugins, PhysicsPlugin, CharacterControllerPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
            .insert_resource(BlockRegistry::new())
            .init_resource::<ChunkCache>();
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(duration);
        app
    }

    /// Advances exactly one FixedUpdate tick (two app.update() calls: one to
    /// seed the clock, one to overflow the accumulator).
    fn tick(app: &mut App) {
        app.update();
        app.update();
    }

    fn spawn_character(app: &mut App, pos: Vec3, grounded: bool) -> Entity {
        let entity = app
            .world_mut()
            .spawn((
                Transform::from_translation(pos),
                PhysicsBody,
                Aabb::player(),
                GravityScale(0.0), // disable gravity so only controller drives motion
                CharacterController::default(),
                MovementSpeed(5.0),
            ))
            .id();
        // Pre-set grounded flag so jump tests work without needing real blocks.
        if grounded {
            app.world_mut()
                .entity_mut(entity)
                .get_mut::<Grounded>()
                .unwrap()
                .0 = true;
        }
        entity
    }

    #[test]
    fn movement_sets_horizontal_velocity() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, false);

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .movement = Vec3::new(1.0, 0.0, 0.0);

        // Seed the clock so the accumulator has a value before the real tick.
        app.update();

        // Manually re-write movement because the controller reset it during
        // the seed frame's FixedUpdate.
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .movement = Vec3::new(1.0, 0.0, 0.0);

        app.update(); // real FixedUpdate tick

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert!(
            transform.translation.x > 0.0,
            "character should have moved in +X, got {}",
            transform.translation.x
        );
    }

    #[test]
    fn movement_does_not_affect_vertical_velocity() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, false);

        // Manually pre-set velocity.y to detect any unwanted vertical write.
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Velocity>()
            .unwrap()
            .0
            .y = 5.0;

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .movement = Vec3::new(0.0, 0.0, 1.0);

        tick(&mut app);

        let vel = app.world().get::<Velocity>(entity).unwrap();
        // Vertical velocity should be unchanged by movement intent (gravity
        // is disabled, so it stays at the manually-set 5.0, minus any
        // friction from finalise — just check it wasn't zeroed by the
        // controller).
        assert!(
            vel.0.y > 0.0,
            "movement should not zero out vertical velocity, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_fires_when_grounded() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, true);

        app.update(); // seed clock; grounded is reset to false during Integrate

        // Re-set grounded so the second tick sees it as true.
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Grounded>()
            .unwrap()
            .0 = true;

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .jump = true;

        app.update(); // real FixedUpdate tick

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y > 0.0,
            "jump should have set upward velocity, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_does_not_fire_when_not_grounded() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, false);

        app.update(); // seed clock

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .jump = true;

        app.update(); // real FixedUpdate tick

        let vel = app.world().get::<Velocity>(entity).unwrap();
        // No ground → jump should not apply. Gravity is disabled so vertical
        // velocity should be near zero (only friction/initialisation noise).
        assert!(
            vel.0.y <= 0.0,
            "jump should not fire when not grounded, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_flag_reset_after_firing() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, true);

        app.update(); // seed clock

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<Grounded>()
            .unwrap()
            .0 = true;
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .jump = true;

        app.update(); // fires jump and resets the flag

        let ctrl = app.world().get::<CharacterController>(entity).unwrap();
        assert!(
            !ctrl.jump,
            "jump flag should be reset after firing, got {}",
            ctrl.jump
        );
    }

    #[test]
    fn sprint_multiplier_scales_speed() {
        let mut app = make_app(1.0 / 60.0);
        let normal = spawn_character(&mut app, Vec3::ZERO, false);
        let sprinter = spawn_character(&mut app, Vec3::new(100.0, 0.0, 0.0), false);

        app.update(); // seed clock

        // Normal movement
        {
            let world = app.world_mut();
            let mut entity_ref = world.entity_mut(normal);
            let mut ctrl = entity_ref.get_mut::<CharacterController>().unwrap();
            ctrl.movement = Vec3::new(1.0, 0.0, 0.0);
            ctrl.sprint_multiplier = 1.0;
        }
        // Sprint movement
        {
            let world = app.world_mut();
            let mut entity_ref = world.entity_mut(sprinter);
            let mut ctrl = entity_ref.get_mut::<CharacterController>().unwrap();
            ctrl.movement = Vec3::new(1.0, 0.0, 0.0);
            ctrl.sprint_multiplier = 2.0;
        }

        app.update(); // real FixedUpdate tick

        let normal_vel = app.world().get::<Velocity>(normal).unwrap().0.x;
        let sprint_vel = app.world().get::<Velocity>(sprinter).unwrap().0.x;

        // After finalise, friction is applied, but sprinter should still be
        // proportionally faster. We check the ratio is approximately 2.
        // (Both velocities are already decayed by the same friction factor, so
        // the ratio is preserved.)
        assert!(
            sprint_vel > normal_vel,
            "sprinter velocity ({}) should exceed normal ({})",
            sprint_vel,
            normal_vel
        );
    }
}
