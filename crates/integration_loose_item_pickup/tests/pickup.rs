//! Integration tests for the loose-item ↔ inventory pickup flow.

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
use dd40_loose_item_core::{LooseItem, PickupCooldown};
use dd40_physics_core::messages::BodyBodyContact;
use dd40_physics_core::plugin::PhysicsCorePlugin;
use dd40_physics_core::prelude::PhysicsPosition;

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

fn register_stone(app: &mut App, max_stack: u16) -> ItemId {
    let mut registry = app.world_mut().resource_mut::<ItemRegistry>();
    registry.register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(max_stack)))
}

fn spawn_character(app: &mut App, pos: Vec3, capacity: usize) -> Entity {
    app.world_mut()
        .spawn((
            Character,
            PhysicsPosition(pos),
            InventoryComponent::with_capacity(capacity),
        ))
        .id()
}

/// Spawn a loose item.  `cooldown_secs == 0` produces a timer that is
/// already finished (eligible for pickup this tick).
fn spawn_loose(app: &mut App, item: ItemId, count: u16, pos: Vec3, cooldown_secs: u64) -> Entity {
    let mut timer = Timer::new(Duration::from_secs(cooldown_secs.max(1)), TimerMode::Once);
    if cooldown_secs == 0 {
        timer.tick(Duration::from_secs(2));
        assert!(timer.is_finished());
    }
    app.world_mut()
        .spawn((
            LooseItem::new(ItemStack::new(item, nz(count))),
            PhysicsPosition(pos),
            PickupCooldown(timer),
        ))
        .id()
}

fn write_contact(app: &mut App, a: Entity, b: Entity) {
    app.world_mut()
        .write_message(BodyBodyContact::new(a, b, Vec3::Y, 0.0));
}

/// Pre-fill the character's first slot with `count` of `item`, using
/// the real [`ItemRegistry`] from the app.
fn prefill_first_slot(app: &mut App, character: Entity, item: ItemId, count: u16) {
    app.world_mut()
        .resource_scope(|world, registry: Mut<ItemRegistry>| {
            let mut inv = world.get_mut::<InventoryComponent>(character).unwrap();
            let _ = inv
                .inventory_mut()
                .insert_stack(ItemStack::new(item, nz(count)), &registry);
        });
}

#[test]
fn picked_up_stack_despawns_loose_entity_and_fills_inventory() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let character = spawn_character(&mut app, Vec3::ZERO, 4);
    let loose = spawn_loose(&mut app, id, 10, Vec3::ZERO, 0);

    write_contact(&mut app, character, loose);
    app.update();

    assert!(
        app.world().get_entity(loose).is_err(),
        "loose entity should have been despawned"
    );
    let inv = app
        .world()
        .get::<InventoryComponent>(character)
        .unwrap()
        .inventory();
    assert_eq!(inv.slot(0).unwrap().count.get(), 10);
}

#[test]
fn pickup_is_skipped_while_cooldown_is_running() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let character = spawn_character(&mut app, Vec3::ZERO, 4);
    let loose = spawn_loose(&mut app, id, 10, Vec3::ZERO, 60);

    write_contact(&mut app, character, loose);
    app.update();

    assert!(app.world().get::<LooseItem>(loose).is_some());
    let inv = app
        .world()
        .get::<InventoryComponent>(character)
        .unwrap()
        .inventory();
    assert!(inv.slot(0).is_none());
}

#[test]
fn partial_pickup_shrinks_loose_stack_to_leftover() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let character = spawn_character(&mut app, Vec3::ZERO, 1);

    // Pre-fill the single slot with 60 stone using the real registry.
    prefill_first_slot(&mut app, character, id, 60);

    let loose = spawn_loose(&mut app, id, 10, Vec3::ZERO, 0);
    write_contact(&mut app, character, loose);
    app.update();

    let loose_view = app
        .world()
        .get::<LooseItem>(loose)
        .expect("partial pickup must leave the entity in place");
    assert_eq!(loose_view.stack.count.get(), 6, "60 + 4 = 64; leftover 6");

    let inv = app
        .world()
        .get::<InventoryComponent>(character)
        .unwrap()
        .inventory();
    assert_eq!(inv.slot(0).unwrap().count.get(), 64);
}

#[test]
fn full_inventory_leaves_loose_entity_untouched() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let character = spawn_character(&mut app, Vec3::ZERO, 1);

    prefill_first_slot(&mut app, character, id, 64);

    let loose = spawn_loose(&mut app, id, 5, Vec3::ZERO, 0);
    write_contact(&mut app, character, loose);
    app.update();

    let loose_view = app.world().get::<LooseItem>(loose).unwrap();
    assert_eq!(loose_view.stack.count.get(), 5, "stack must be untouched");
}

#[test]
fn nearest_character_wins_multi_pickup_tie_break() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let near = spawn_character(&mut app, Vec3::new(0.5, 0.0, 0.0), 4);
    let far = spawn_character(&mut app, Vec3::new(5.0, 0.0, 0.0), 4);
    let loose = spawn_loose(&mut app, id, 3, Vec3::ZERO, 0);

    write_contact(&mut app, near, loose);
    write_contact(&mut app, far, loose);
    app.update();

    assert!(
        app.world().get_entity(loose).is_err(),
        "loose entity should have been picked up"
    );
    let near_inv = app
        .world()
        .get::<InventoryComponent>(near)
        .unwrap()
        .inventory();
    let far_inv = app
        .world()
        .get::<InventoryComponent>(far)
        .unwrap()
        .inventory();
    assert_eq!(near_inv.slot(0).unwrap().count.get(), 3);
    assert!(far_inv.slot(0).is_none());
}

#[test]
fn equal_distance_tie_break_goes_to_lower_entity_index() {
    let mut app = new_app();
    let id = register_stone(&mut app, 64);
    let a = spawn_character(&mut app, Vec3::new(1.0, 0.0, 0.0), 4);
    let b = spawn_character(&mut app, Vec3::new(-1.0, 0.0, 0.0), 4);
    let loose = spawn_loose(&mut app, id, 2, Vec3::ZERO, 0);

    write_contact(&mut app, a, loose);
    write_contact(&mut app, b, loose);
    app.update();

    let (lower, higher) = if a.index() < b.index() {
        (a, b)
    } else {
        (b, a)
    };
    let lower_inv = app
        .world()
        .get::<InventoryComponent>(lower)
        .unwrap()
        .inventory();
    let higher_inv = app
        .world()
        .get::<InventoryComponent>(higher)
        .unwrap()
        .inventory();
    assert_eq!(
        lower_inv.slot(0).unwrap().count.get(),
        2,
        "lower-index character should win the tie"
    );
    assert!(higher_inv.slot(0).is_none());
}
