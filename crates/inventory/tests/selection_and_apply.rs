//! Integration tests for the vanilla inventory rules crate.
//!
//! Focused on behaviour reachable without driving real `bevy_enhanced_input`
//! action presses: mouse-wheel selection (via `SetActiveSlot`), drop-held
//! intent, and the `ActiveItem` provider attach/refresh.

use std::num::NonZero;

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_inventory::{InventoryActiveItemPlugin, InventoryPlugin, InventoryRulesPlugin};
use dd40_inventory_core::prelude::{
    DropItems, HOTBAR_SIZE, HeldStackComponent, InventoryComponent, SlotInteraction,
    SlotInteractionKind,
};
use dd40_item_core::active_item::{ActiveItem, ItemStack};
use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry};

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<MouseWheel>();
    app.add_plugins((
        InventoryActiveItemPlugin,
        InventoryPlugin,
        InventoryRulesPlugin,
    ));
    app
}

fn nz(n: u16) -> NonZero<u16> {
    NonZero::new(n).unwrap()
}

#[test]
fn mouse_wheel_shifts_active_slot_and_wraps() {
    let mut app = make_app();
    let player = app
        .world_mut()
        .spawn((
            Player,
            InventoryComponent::with_capacity(HOTBAR_SIZE as usize),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world()
            .get::<InventoryComponent>(player)
            .unwrap()
            .inventory()
            .active_slot(),
        0
    );

    // Scroll up one line → moves left → wraps to last slot.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<MouseWheel>>()
        .write(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: 1.0,
            window: Entity::PLACEHOLDER,
        });
    // Two updates: tick 1 emits SetActiveSlot, tick 2 applies it.
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .get::<InventoryComponent>(player)
            .unwrap()
            .inventory()
            .active_slot(),
        HOTBAR_SIZE - 1
    );

    // Scroll down one line → moves right → wraps back to 0.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<MouseWheel>>()
        .write(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -1.0,
            window: Entity::PLACEHOLDER,
        });
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .get::<InventoryComponent>(player)
            .unwrap()
            .inventory()
            .active_slot(),
        0
    );
}

#[test]
fn drop_held_clears_held_and_emits_drop_items() {
    let mut app = make_app();
    let stack = ItemStack::new(ItemId(1), nz(3));
    let player = app
        .world_mut()
        .spawn((
            Player,
            InventoryComponent::with_capacity(9),
            HeldStackComponent(Some(stack)),
            Transform::default(),
        ))
        .id();
    app.update();

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<SlotInteraction>>()
        .write(SlotInteraction {
            character: player,
            kind: SlotInteractionKind::DropHeld,
        });
    app.update();

    assert!(
        app.world()
            .get::<HeldStackComponent>(player)
            .map(|h| h.is_empty())
            .unwrap_or(false),
        "HeldStackComponent on player must be empty after DropHeld",
    );
    let drops = app
        .world()
        .resource::<bevy::ecs::message::Messages<DropItems>>();
    let collected: Vec<_> = drops.iter_current_update_messages().cloned().collect();
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].stacks, vec![stack]);
}

#[test]
fn active_item_cache_mirrors_active_slot_contents() {
    let mut app = make_app();
    app.world_mut()
        .resource_mut::<ItemRegistry>()
        .register(ItemDefinition::new(ItemId(1), "test"));
    let stack = ItemStack::new(ItemId(1), nz(1));
    let mut inv = InventoryComponent::with_capacity(9);
    inv.inventory_mut().set_slot(2, Some(stack));
    let player = app.world_mut().spawn((Player, inv)).id();
    // tick 1: attach ActiveItem (provider points at the inventory).
    app.update();
    // tick 2: refresh runs because InventoryComponent is `Added` (also
    // counts as Changed) and pulls cache from slot 0 → None.
    app.update();
    let active = app.world().get::<ActiveItem>(player).unwrap();
    assert!(active.peek().is_none(), "slot 0 is empty");

    // Move active slot to 2.
    app.world_mut()
        .get_mut::<InventoryComponent>(player)
        .unwrap()
        .set_active_slot(2);
    app.update();
    let active = app.world().get::<ActiveItem>(player).unwrap();
    assert_eq!(active.peek(), Some(stack));
}

#[test]
fn active_item_cache_refreshes_on_slot_mutation() {
    let mut app = make_app();
    app.world_mut()
        .resource_mut::<ItemRegistry>()
        .register(ItemDefinition::new(ItemId(7), "gold"));
    let player = app
        .world_mut()
        .spawn((Player, InventoryComponent::with_capacity(9)))
        .id();
    app.update();
    app.update();

    // Mutate slot 0 via the event-firing setter so the inventory marks
    // itself `Changed` and the refresh system picks it up.
    let stack = ItemStack::new(ItemId(7), nz(2));
    let sys = app.register_system(
        move |mut q: Query<&mut InventoryComponent>,
              mut commands: Commands,
              players: Query<Entity, With<Player>>| {
            let player = players.single().unwrap();
            let mut inv = q.get_mut(player).unwrap();
            inv.set_slot(0, Some(stack), &mut commands, player);
        },
    );
    app.world_mut().run_system(sys).unwrap();
    app.update();

    let active = app.world().get::<ActiveItem>(player).unwrap();
    assert_eq!(active.peek(), Some(stack));
}

#[test]
fn remote_character_without_player_still_gets_active_item() {
    // Server-authoritative invariant: every entity with an
    // `InventoryComponent` gets an `ActiveItem` (so the server can
    // resolve placement / mining for remote characters too).  The
    // `Player` marker only gates *input* — number-key and wheel
    // routing — not the active-item attach.
    use dd40_character_core::components::Character;

    let mut app = make_app();
    let local = app
        .world_mut()
        .spawn((Player, InventoryComponent::with_capacity(9)))
        .id();
    let remote = app
        .world_mut()
        .spawn((Character, InventoryComponent::with_capacity(9)))
        .id();
    app.update();

    assert!(
        app.world().get::<ActiveItem>(local).is_some(),
        "local Player must auto-attach ActiveItem",
    );
    assert!(
        app.world().get::<ActiveItem>(remote).is_some(),
        "remote Character with InventoryComponent must also auto-attach ActiveItem",
    );

    // Scroll once: only the local player's active_slot should change.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<MouseWheel>>()
        .write(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -1.0,
            window: Entity::PLACEHOLDER,
        });
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .get::<InventoryComponent>(local)
            .unwrap()
            .inventory()
            .active_slot(),
        1
    );
    assert_eq!(
        app.world()
            .get::<InventoryComponent>(remote)
            .unwrap()
            .inventory()
            .active_slot(),
        0,
        "remote character's active_slot must remain unchanged after local scroll",
    );
}
