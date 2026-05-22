//! Integration tests for the loose-item attraction stage.

use std::num::NonZero;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::{Timer, TimerMode};

use dd40_character_core::components::Character;
use dd40_integration_loose_item_pickup::LooseItemPickupPlugin;
use dd40_inventory_core::component::InventoryComponent;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry};
use dd40_loose_item_core::plugin::LooseItemCorePlugin;
use dd40_loose_item_core::{LooseItem, LooseItemConfig, PickupCooldown};
use dd40_physics_core::components::{Impulse, PhysicsPosition};
use dd40_physics_core::plugin::PhysicsCorePlugin;

fn nz(n: u16) -> NonZero<u16> {
    NonZero::new(n).expect("non-zero literal")
}

fn new_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        ItemCorePlugin,
        PhysicsCorePlugin,
        LooseItemCorePlugin,
        LooseItemPickupPlugin,
    ));
    app
}

fn register_stone(app: &mut App) -> ItemId {
    let mut registry = app.world_mut().resource_mut::<ItemRegistry>();
    registry.register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(64)))
}

fn spawn_character(app: &mut App, pos: Vec3, capacity: usize) -> Entity {
    app.world_mut()
        .spawn((
            Character,
            Transform::from_translation(pos),
            PhysicsPosition(pos),
            InventoryComponent::with_capacity(capacity),
        ))
        .id()
}

fn spawn_loose(app: &mut App, item: ItemId, pos: Vec3) -> Entity {
    let mut timer = Timer::new(Duration::from_secs(1), TimerMode::Once);
    timer.tick(Duration::from_secs(2));
    app.world_mut()
        .spawn((
            LooseItem::new(ItemStack::new(item, nz(1))),
            Transform::from_translation(pos),
            PhysicsPosition(pos),
            PickupCooldown(timer),
            Impulse::default(),
        ))
        .id()
}

fn fill_first_slot(app: &mut App, character: Entity, item: ItemId, count: u16) {
    let mut inv = app
        .world_mut()
        .get_mut::<InventoryComponent>(character)
        .unwrap();
    let _ = inv
        .inventory_mut()
        .insert_stack_strict(0, ItemStack::new(item, nz(count)));
}

fn advance(app: &mut App, dur: Duration) {
    app.world_mut()
        .resource_mut::<bevy::time::Time<bevy::time::Virtual>>()
        .set_max_delta(Duration::MAX);
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(dur));
    // First update after inserting the strategy still observes dt=0; tick
    // twice so the system sees the requested duration.
    app.update();
    app.update();
}

fn impulse_of(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Impulse>(e).copied().unwrap().0
}

#[test]
fn item_inside_radius_gains_impulse_toward_character() {
    let mut app = new_app();
    let item = register_stone(&mut app);
    let _char = spawn_character(&mut app, Vec3::ZERO, 4);
    let loose = spawn_loose(&mut app, item, Vec3::new(1.0, 0.0, 0.0));
    app.update(); // dt=0, no impulse
    assert_eq!(impulse_of(&app, loose), Vec3::ZERO);

    advance(&mut app, Duration::from_millis(50));
    let imp = impulse_of(&app, loose);
    assert!(imp.x < 0.0, "should accelerate toward origin, got {imp:?}");
    assert_eq!(imp.y, 0.0);
    assert_eq!(imp.z, 0.0);
}

#[test]
fn item_outside_radius_is_not_attracted() {
    let mut app = new_app();
    let item = register_stone(&mut app);
    let _char = spawn_character(&mut app, Vec3::ZERO, 4);
    // Default radius is 1.5
    let loose = spawn_loose(&mut app, item, Vec3::new(5.0, 0.0, 0.0));
    advance(&mut app, Duration::from_millis(50));
    assert_eq!(impulse_of(&app, loose), Vec3::ZERO);
}

#[test]
fn full_inventory_disables_attraction() {
    let mut app = new_app();
    let item = register_stone(&mut app);
    let character = spawn_character(&mut app, Vec3::ZERO, 1);
    fill_first_slot(&mut app, character, item, 64);
    let loose = spawn_loose(&mut app, item, Vec3::new(1.0, 0.0, 0.0));
    advance(&mut app, Duration::from_millis(50));
    assert_eq!(impulse_of(&app, loose), Vec3::ZERO);
}

#[test]
fn pickup_cooldown_blocks_attraction() {
    let mut app = new_app();
    let item = register_stone(&mut app);
    let _char = spawn_character(&mut app, Vec3::ZERO, 4);
    let loose = app
        .world_mut()
        .spawn((
            LooseItem::new(ItemStack::new(item, nz(1))),
            PhysicsPosition(Vec3::new(1.0, 0.0, 0.0)),
            PickupCooldown(Timer::new(Duration::from_secs(10), TimerMode::Once)),
            Impulse::default(),
        ))
        .id();
    advance(&mut app, Duration::from_millis(50));
    assert_eq!(impulse_of(&app, loose), Vec3::ZERO);
}

#[test]
fn nearest_character_with_space_wins() {
    let mut app = new_app();
    let item = register_stone(&mut app);
    let _far = spawn_character(&mut app, Vec3::new(-1.4, 0.0, 0.0), 4);
    let _near = spawn_character(&mut app, Vec3::new(0.5, 0.0, 0.0), 4);
    let loose = spawn_loose(&mut app, item, Vec3::ZERO);
    advance(&mut app, Duration::from_millis(20));
    let imp = impulse_of(&app, loose);
    assert!(imp.x > 0.0, "should pull toward +x neighbor, got {imp:?}");
}

#[test]
fn zero_radius_disables_attraction() {
    let mut app = new_app();
    app.insert_resource(LooseItemConfig {
        attraction_radius: 0.0,
        ..Default::default()
    });
    let item = register_stone(&mut app);
    let _char = spawn_character(&mut app, Vec3::ZERO, 4);
    let loose = spawn_loose(&mut app, item, Vec3::new(0.5, 0.0, 0.0));
    advance(&mut app, Duration::from_millis(50));
    assert_eq!(impulse_of(&app, loose), Vec3::ZERO);
}
