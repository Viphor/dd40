use std::time::Duration;

use bevy::prelude::*;
use bevy::time::{Fixed, TimeUpdateStrategy};
use dd40_core::{block::BlockRegistry, chunk::cache::ChunkCache};
use dd40_physics::plugin::PhysicsPlugin;
use dd40_physics_core::components::{Aabb, CharacterPosition, PhysicsBody};

fn make_app(dt_secs: f32) -> App {
    let duration = Duration::from_secs_f32(dt_secs);
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins)
        .add_plugins(PhysicsPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(duration))
        .insert_resource(BlockRegistry::new())
        .init_resource::<ChunkCache>();
    app.world_mut()
        .resource_mut::<Time<Fixed>>()
        .set_timestep(duration);
    app
}

/// Advance the app by one fixed-timestep tick.
///
/// Two [`App::update`] calls are required: the first lets Bevy's scheduler
/// accumulate the manual duration into the `Time<Fixed>` bucket; the second
/// actually drains that bucket and runs the `FixedUpdate` schedule where the
/// physics systems live. With only one call the fixed schedule never fires.
fn tick(app: &mut App) {
    app.update();
    app.update();
}

#[test]
fn physics_body_falls_under_gravity() {
    let mut app = make_app(0.016);

    let entity = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(0.0, 100.0, 0.0)),
            PhysicsBody,
            Aabb::player(),
        ))
        .id();

    for _ in 0..5 {
        tick(&mut app);
    }

    let pos = *app.world().entity(entity).get::<CharacterPosition>().unwrap();
    assert!(
        pos.0.y < 100.0,
        "gravity should have moved entity downward, y={:.3}",
        pos.0.y
    );
}

#[test]
fn physics_plugin_auto_adds_dependencies() {
    // Neither CorePlugin nor PhysicsCorePlugin is added manually.
    let mut app = App::new();
    app.add_plugins(bevy::MinimalPlugins).add_plugins(PhysicsPlugin);
    // BlockRegistry is inserted by CorePlugin — proves ensure_plugins! ran.
    assert!(app.world().contains_resource::<BlockRegistry>());
    // PhysicsConfig is inserted by PhysicsCorePlugin.
    assert!(app.world().contains_resource::<dd40_physics_core::components::PhysicsConfig>());
}
