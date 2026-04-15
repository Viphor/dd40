//! Character controller — the bridge between input systems and the physics pipeline.
//!
//! Any system (player input, AI, network replication) that wants to move a
//! character writes its intent into the [`CharacterController`] component.  A
//! single physics-side system ([`apply_character_controller`]) then translates
//! that intent into [`Impulse`] changes before [`PhysicsSet::Integrate`] runs,
//! so gravity, block collision, and character-vs-character collision all act on
//! the resulting velocity as normal.
//!
//! # Movement model
//!
//! Rather than setting [`Velocity`] directly, the controller adds a *correction
//! impulse* each tick:
//!
//! ```text
//! correction = target_horizontal_velocity − current_horizontal_velocity
//! impulse   += correction × (grounded ? 1.0 : air_control)
//! ```
//!
//! When grounded, the full correction is applied so movement feels snappy and
//! responsive.  High ground friction then quickly zeroes residual velocity when
//! the player releases a key.  In the air, only a fraction (`air_control`) of
//! the correction is applied per tick, so direction changes are gradual and the
//! player carries their existing momentum — consistent with typical platformer
//! feel.
//!
//! # Jumping
//!
//! Jumping is **opt-in**: the entity must also have a [`JumpImpulse`] component.
//! Without it, `controller.jump = true` is silently ignored.  This prevents
//! non-player physics bodies from gaining jump capability.
//!
//! # Usage
//!
//! 1. Insert [`CharacterController`] and [`JumpImpulse`] alongside [`PhysicsBody`]
//!    when spawning a jumpable character.
//! 2. Each frame, write desired movement and jump intent into [`CharacterController`]
//!    from whichever input system owns the character.
//! 3. Do **not** mutate [`Velocity`] or [`Impulse`] directly for locomotion.
//!
//! ```
//! use dd40_core::character::controller::CharacterController;
//! use bevy::math::Vec3;
//!
//! fn move_character(mut query: bevy::prelude::Query<&mut CharacterController>) {
//!     for mut ctrl in &mut query {
//!         ctrl.movement = Vec3::new(1.0, 0.0, 0.0); // move right
//!         ctrl.jump = true;
//!     }
//! }
//! ```
//!
//! [`JumpImpulse`]: crate::character::JumpImpulse
//! [`Velocity`]: crate::character::physics::Velocity
//! [`Impulse`]: crate::character::physics::Impulse

use bevy::prelude::*;

use super::{JumpImpulse, MovementSpeed, physics::{Grounded, Impulse, PhysicsBody, PhysicsSet, Velocity}};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Per-frame movement intent for a character.
///
/// Written to by input systems (player, AI, network) and consumed once per
/// [`FixedUpdate`] tick by [`apply_character_controller`].
///
/// All fields are reset after being consumed each tick:
/// - [`movement`] is applied every tick it is non-zero, then cleared.
/// - [`jump`] is reset to `false` after being processed.
/// - [`sprint_multiplier`] is reset to `1.0` each tick.
///
/// [`movement`]: CharacterController::movement
/// [`jump`]: CharacterController::jump
/// [`sprint_multiplier`]: CharacterController::sprint_multiplier
#[derive(Debug, Clone, Component, Reflect)]
#[reflect(Component)]
pub struct CharacterController {
    /// Desired movement direction in **world space**, projected onto the
    /// horizontal (XZ) plane and normalised.
    ///
    /// The controller multiplies this by [`MovementSpeed`] and
    /// [`sprint_multiplier`] to compute the target horizontal velocity, then
    /// adds a correction impulse toward that target.
    pub movement: Vec3,

    /// When `true`, the controller attempts to jump this tick.
    ///
    /// Requires a [`JumpImpulse`] component on the same entity — without it
    /// the request is silently dropped.  Reset to `false` immediately after
    /// being processed.
    ///
    /// [`JumpImpulse`]: crate::character::JumpImpulse
    pub jump: bool,

    /// Scale factor applied to [`MovementSpeed`] this tick.
    ///
    /// `1.0` = normal speed, `2.0` = sprinting.  Reset to `1.0` each tick.
    pub sprint_multiplier: f32,

    /// Fraction of the movement correction impulse applied when the entity is
    /// **not** grounded.
    ///
    /// `1.0` = full air control (same as ground), `0.0` = no air steering.
    /// Typical values are `0.2`–`0.4`.
    pub air_control: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            movement: Vec3::ZERO,
            jump: false,
            sprint_multiplier: 1.0,
            air_control: 0.3,
        }
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// Translates [`CharacterController`] intent into [`Impulse`] changes.
///
/// Runs in [`FixedUpdate`] **before** [`PhysicsSet::Integrate`] so the impulse
/// is flushed into [`Velocity`] during integration that same tick.
///
/// ### Ground vs. air movement
///
/// When grounded, the full velocity correction is applied as an impulse so
/// movement feels snappy.  When airborne, only `air_control × correction` is
/// applied, preserving momentum and making mid-air direction changes gradual.
///
/// ### Jump
///
/// A jump impulse is added to `impulse.0.y` only when **all** of:
/// - `controller.jump` is `true`
/// - the entity has a [`JumpImpulse`] component
/// - the entity is [`Grounded`]
///
/// The jump flag is always reset after processing regardless of whether the
/// jump actually fired.
fn apply_character_controller(
    mut query: Query<
        (
            &mut CharacterController,
            &MovementSpeed,
            &Grounded,
            &Velocity,
            &mut Impulse,
            Option<&JumpImpulse>,
        ),
        With<PhysicsBody>,
    >,
) {
    for (mut controller, speed, grounded, velocity, mut impulse, jump_impulse) in &mut query {
        // ── Horizontal movement ───────────────────────────────────────────
        let target_h = Vec3::new(
            controller.movement.x * speed.0 * controller.sprint_multiplier,
            0.0,
            controller.movement.z * speed.0 * controller.sprint_multiplier,
        );
        let current_h = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
        let correction = target_h - current_h;

        let factor = if grounded.is_grounded() {
            1.0
        } else {
            controller.air_control
        };

        impulse.0.x += correction.x * factor;
        impulse.0.z += correction.z * factor;

        // ── Jump ─────────────────────────────────────────────────────────
        if controller.jump {
            if grounded.is_grounded() {
                if let Some(ji) = jump_impulse {
                    impulse.0.y += ji.0;
                }
            }
            // Always consume the flag so a held key doesn't re-fire next frame.
            controller.jump = false;
        }

        // ── Reset per-frame intent ────────────────────────────────────────
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
        character::{JumpImpulse, physics::{Aabb, GravityScale, PhysicsPlugin}},
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

    /// Advances exactly one FixedUpdate tick (two app.update() calls).
    fn tick(app: &mut App) {
        app.update(); // seed real-time clock
        app.update(); // overflow accumulator → one FixedUpdate tick
    }

    fn spawn_character(app: &mut App, pos: Vec3, grounded: bool, with_jump: bool) -> Entity {
        let mut cmd = app.world_mut().spawn((
            Transform::from_translation(pos),
            PhysicsBody,
            Aabb::player(),
            GravityScale(0.0), // disable gravity so only the controller drives motion
            CharacterController::default(),
            MovementSpeed(5.0),
        ));
        if with_jump {
            cmd.insert(JumpImpulse::default());
        }
        let entity = cmd.id();
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
        let entity = spawn_character(&mut app, Vec3::ZERO, true, false);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(entity);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            entity_ref.get_mut::<CharacterController>().unwrap().movement =
                Vec3::new(1.0, 0.0, 0.0);
        }

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
        let entity = spawn_character(&mut app, Vec3::ZERO, false, false);

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
        assert!(
            vel.0.y > 0.0,
            "movement should not zero out vertical velocity, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_fires_when_grounded_and_has_jump_impulse() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, true, true);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(entity);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            entity_ref.get_mut::<CharacterController>().unwrap().jump = true;
        }

        app.update(); // real FixedUpdate tick

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y > 0.0,
            "jump should have set upward velocity, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_ignored_without_jump_impulse_component() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, true, false);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(entity);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            entity_ref.get_mut::<CharacterController>().unwrap().jump = true;
        }

        app.update(); // real FixedUpdate tick

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y <= 0.0,
            "jump should be ignored without JumpImpulse component, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_does_not_fire_when_not_grounded() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, false, true);

        app.update(); // seed clock

        app.world_mut()
            .entity_mut(entity)
            .get_mut::<CharacterController>()
            .unwrap()
            .jump = true;

        app.update(); // real FixedUpdate tick

        let vel = app.world().get::<Velocity>(entity).unwrap();
        assert!(
            vel.0.y <= 0.0,
            "jump should not fire when not grounded, got {}",
            vel.0.y
        );
    }

    #[test]
    fn jump_flag_reset_after_processing() {
        let mut app = make_app(1.0 / 60.0);
        let entity = spawn_character(&mut app, Vec3::ZERO, true, true);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(entity);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            entity_ref.get_mut::<CharacterController>().unwrap().jump = true;
        }

        app.update(); // fires jump and resets the flag

        let ctrl = app.world().get::<CharacterController>(entity).unwrap();
        assert!(!ctrl.jump, "jump flag should be reset after processing");
    }

    #[test]
    fn air_control_reduces_horizontal_impulse() {
        let mut app = make_app(1.0 / 20.0);
        let grounded = spawn_character(&mut app, Vec3::ZERO, true, false);
        let airborne = spawn_character(&mut app, Vec3::new(100.0, 0.0, 0.0), false, false);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(grounded);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            entity_ref.get_mut::<CharacterController>().unwrap().movement =
                Vec3::new(1.0, 0.0, 0.0);
        }
        {
            let mut entity_ref = app.world_mut().entity_mut(airborne);
            entity_ref.get_mut::<CharacterController>().unwrap().movement =
                Vec3::new(1.0, 0.0, 0.0);
        }

        app.update(); // real FixedUpdate tick

        let grounded_vel = app.world().get::<Velocity>(grounded).unwrap().0.x;
        let airborne_vel = app.world().get::<Velocity>(airborne).unwrap().0.x;

        assert!(
            grounded_vel > airborne_vel,
            "grounded ({}) should move faster than airborne ({}) due to air_control",
            grounded_vel,
            airborne_vel
        );
    }

    #[test]
    fn sprint_multiplier_scales_velocity() {
        let mut app = make_app(1.0 / 20.0);
        let normal = spawn_character(&mut app, Vec3::ZERO, true, false);
        let sprinter = spawn_character(&mut app, Vec3::new(100.0, 0.0, 0.0), true, false);

        app.update(); // seed clock

        {
            let mut entity_ref = app.world_mut().entity_mut(normal);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            let mut ctrl = entity_ref.get_mut::<CharacterController>().unwrap();
            ctrl.movement = Vec3::new(1.0, 0.0, 0.0);
            ctrl.sprint_multiplier = 1.0;
        }
        {
            let mut entity_ref = app.world_mut().entity_mut(sprinter);
            entity_ref.get_mut::<Grounded>().unwrap().0 = true;
            let mut ctrl = entity_ref.get_mut::<CharacterController>().unwrap();
            ctrl.movement = Vec3::new(1.0, 0.0, 0.0);
            ctrl.sprint_multiplier = 2.0;
        }

        app.update(); // real FixedUpdate tick

        let normal_vel = app.world().get::<Velocity>(normal).unwrap().0.x;
        let sprint_vel = app.world().get::<Velocity>(sprinter).unwrap().0.x;

        assert!(
            sprint_vel > normal_vel,
            "sprinter ({}) should be faster than normal ({})",
            sprint_vel,
            normal_vel
        );
    }
}
