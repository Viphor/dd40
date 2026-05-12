//! End-to-end integration tests for [`IntegrationCharacterPhysicsPlugin`]
//! against the real [`PhysicsPlugin`].
//!
//! The unit tests in `src/plugin.rs` exercise the controller system in
//! isolation; this file exercises it together with gravity, integration,
//! and grounded detection — the full character-locomotion pipeline.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::{Fixed, TimeUpdateStrategy};
use dd40_character_core::{
    components::{JumpImpulse, MovementSpeed},
    controller::{CharacterController, CharacterInput},
};
use dd40_core::chunk::cache::ChunkCache;
use dd40_integration_character_physics::IntegrationCharacterPhysicsPlugin;
use dd40_physics::plugin::PhysicsPlugin;
use dd40_physics_core::prelude::{Aabb, GravityScale, Grounded, PhysicsBody, Velocity};

fn make_app(dt_secs: f32) -> App {
    let duration = Duration::from_secs_f32(dt_secs);
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins)
        .add_plugins((PhysicsPlugin, IntegrationCharacterPhysicsPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
        .init_resource::<ChunkCache>();
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .set_timestep(duration);
    app
}

fn spawn_character(app: &mut App, pos: Vec3, grounded: bool, with_jump: bool) -> Entity {
    let mut cmd = app.world_mut().spawn((
        Transform::from_translation(pos),
        PhysicsBody,
        Aabb::player(),
        GravityScale(0.0),
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

    app.update(); // seed clock — FixedUpdate fires once (no input yet)

    {
        let mut entity_ref = app.world_mut().entity_mut(entity);
        entity_ref.get_mut::<Grounded>().unwrap().0 = true;
        entity_ref.get_mut::<CharacterInput>().unwrap().movement = Vec3::new(1.0, 0.0, 0.0);
    }

    app.update(); // FixedUpdate fires: controller applies impulse, integrate moves entity

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
        .get_mut::<CharacterInput>()
        .unwrap()
        .movement = Vec3::new(0.0, 0.0, 1.0);

    app.update();
    app.update();

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
        entity_ref.get_mut::<CharacterInput>().unwrap().jump = true;
    }

    app.update(); // FixedUpdate: controller fires jump impulse, integrate converts to velocity

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
        entity_ref.get_mut::<CharacterInput>().unwrap().jump = true;
    }

    app.update();

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
        .get_mut::<CharacterInput>()
        .unwrap()
        .jump = true;

    app.update();

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
        entity_ref.get_mut::<CharacterInput>().unwrap().jump = true;
    }

    app.update(); // controller resets jump flag regardless of whether jump fires

    let input = app.world().get::<CharacterInput>(entity).unwrap();
    assert!(!input.jump, "jump flag should be reset after processing");
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
        entity_ref.get_mut::<CharacterInput>().unwrap().movement = Vec3::new(1.0, 0.0, 0.0);
    }
    {
        app.world_mut()
            .entity_mut(airborne)
            .get_mut::<CharacterInput>()
            .unwrap()
            .movement = Vec3::new(1.0, 0.0, 0.0);
    }

    app.update();

    let grounded_vel = app.world().get::<Velocity>(grounded).unwrap().0.x;
    let airborne_vel = app.world().get::<Velocity>(airborne).unwrap().0.x;

    assert!(
        grounded_vel > airborne_vel,
        "grounded ({:.4}) should move faster than airborne ({:.4}) due to air_control",
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
        let mut ci = entity_ref.get_mut::<CharacterInput>().unwrap();
        ci.movement = Vec3::new(1.0, 0.0, 0.0);
        ci.sprint = false;
    }
    {
        let mut entity_ref = app.world_mut().entity_mut(sprinter);
        entity_ref.get_mut::<Grounded>().unwrap().0 = true;
        let mut ci = entity_ref.get_mut::<CharacterInput>().unwrap();
        ci.movement = Vec3::new(1.0, 0.0, 0.0);
        ci.sprint = true;
    }

    app.update();

    let normal_vel = app.world().get::<Velocity>(normal).unwrap().0.x;
    let sprint_vel = app.world().get::<Velocity>(sprinter).unwrap().0.x;

    assert!(
        sprint_vel > normal_vel,
        "sprinter ({:.4}) should be faster than normal ({:.4})",
        sprint_vel,
        normal_vel
    );
}
