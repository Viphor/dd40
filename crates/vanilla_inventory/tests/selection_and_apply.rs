//! Integration tests for the vanilla inventory rules crate.
//!
//! Focused on behaviour reachable without driving real `bevy_enhanced_input`
//! action presses: mouse-wheel selection, drop-outside intent, and the
//! `ActiveItem` sync observer.

use std::num::NonZero;

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_inventory_core::prelude::{
    DropItems, HOTBAR_SIZE, HeldStackComponent, InventoryChanged, InventoryComponent,
    SelectedHotbarSlot, SlotChange, SlotInteraction, SlotInteractionKind,
};
use dd40_item_core::active_item::{ActiveItem, ItemStack};
use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry};
use dd40_vanilla_inventory::VanillaInventoryPlugin;

fn make_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // The selection systems read `MouseWheel`; ensure the message is
    // registered without pulling in the full `InputPlugin`.
    app.add_message::<MouseWheel>();
    app.add_plugins(VanillaInventoryPlugin);
    app
}

fn nz(n: u16) -> NonZero<u16> {
    NonZero::new(n).unwrap()
}

#[test]
fn mouse_wheel_shifts_hotbar_selection_and_wraps() {
    let mut app = make_app();
    let player = app.world_mut().spawn(Player).id();
    // Let `ensure_selected_slot` insert the component.
    app.update();
    assert_eq!(
        app.world().get::<SelectedHotbarSlot>(player).copied(),
        Some(SelectedHotbarSlot(0))
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
    app.update();
    assert_eq!(
        app.world().get::<SelectedHotbarSlot>(player).copied(),
        Some(SelectedHotbarSlot(HOTBAR_SIZE - 1))
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
    assert_eq!(
        app.world().get::<SelectedHotbarSlot>(player).copied(),
        Some(SelectedHotbarSlot(0))
    );
}

#[test]
fn drop_outside_clears_held_and_emits_drop_items() {
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
    app.update(); // attach SelectedHotbarSlot + ActiveItem.

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<SlotInteraction>>()
        .write(SlotInteraction {
            character: player,
            kind: SlotInteractionKind::DropOutside,
        });
    app.update();

    assert!(
        app.world()
            .get::<HeldStackComponent>(player)
            .map(|h| h.is_empty())
            .unwrap_or(false),
        "HeldStackComponent on player must be empty after DropOutside",
    );
    let drops = app
        .world()
        .resource::<bevy::ecs::message::Messages<DropItems>>();
    let collected: Vec<_> = drops.iter_current_update_messages().cloned().collect();
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].stacks, vec![stack]);
}

#[test]
fn active_item_updates_when_selected_slot_changes() {
    let mut app = make_app();
    // Register the item so registry lookups don't matter; we only need
    // ActiveItem to mirror the slot.
    app.world_mut()
        .resource_mut::<ItemRegistry>()
        .register(ItemDefinition::new(ItemId(1), "test"));
    let stack = ItemStack::new(ItemId(1), nz(1));
    let mut inv = InventoryComponent::with_capacity(9);
    inv.inventory_mut().set_slot(2, Some(stack));
    let player = app.world_mut().spawn((Player, inv)).id();
    app.update(); // attach SelectedHotbarSlot=0 + ActiveItem=None.
    assert_eq!(app.world().get::<ActiveItem>(player).copied(), Some(ActiveItem(None)));

    app.world_mut()
        .get_mut::<SelectedHotbarSlot>(player)
        .unwrap()
        .0 = 2;
    app.update();
    assert_eq!(
        app.world().get::<ActiveItem>(player).copied(),
        Some(ActiveItem(Some(stack)))
    );
}

#[test]
fn active_item_observer_reacts_to_slot_mutation() {
    let mut app = make_app();
    app.world_mut()
        .resource_mut::<ItemRegistry>()
        .register(ItemDefinition::new(ItemId(7), "gold"));
    let player = app
        .world_mut()
        .spawn((Player, InventoryComponent::with_capacity(9)))
        .id();
    app.update();

    // Mutate slot 0 via the component's event-emitting setter so the
    // observer fires.
    let stack = ItemStack::new(ItemId(7), nz(2));
    // We need a Commands buffer; use a one-shot system.
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

    assert_eq!(
        app.world().get::<ActiveItem>(player).copied(),
        Some(ActiveItem(Some(stack)))
    );
    // Silence unused-import warnings if observer-only paths grow later.
    let _ = std::any::TypeId::of::<InventoryChanged>();
    let _ = std::any::TypeId::of::<SlotChange>();
}

#[test]
fn remote_character_without_player_marker_gets_no_hotbar_bookkeeping() {
    // Multiplayer invariant: only the local `Player` entity should ever
    // receive auto-attached hotbar state (`SelectedHotbarSlot`,
    // `ActiveItem`).  Remote characters that arrive via replication carry
    // `Character` + `InventoryComponent` but never the `Player` marker
    // (see `with_predicted_local_player` in `dd40_network`).
    //
    // This test spawns one local player and one "remote" character
    // (Character + InventoryComponent, no Player) and asserts:
    //   1. The local player gets SelectedHotbarSlot + ActiveItem.
    //   2. The remote character gets neither.
    //   3. A mouse-wheel scroll only mutates the local player's slot.
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

    assert_eq!(
        app.world().get::<SelectedHotbarSlot>(local).copied(),
        Some(SelectedHotbarSlot(0)),
        "local Player must auto-attach SelectedHotbarSlot",
    );
    assert!(
        app.world().get::<ActiveItem>(local).is_some(),
        "local Player must auto-attach ActiveItem",
    );
    assert!(
        app.world().get::<SelectedHotbarSlot>(remote).is_none(),
        "remote Character without Player must NOT get SelectedHotbarSlot",
    );
    assert!(
        app.world().get::<ActiveItem>(remote).is_none(),
        "remote Character without Player must NOT get ActiveItem",
    );

    // Scroll once: only the local player's slot should change.
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<MouseWheel>>()
        .write(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: -1.0,
            window: Entity::PLACEHOLDER,
        });
    app.update();
    assert_eq!(
        app.world().get::<SelectedHotbarSlot>(local).copied(),
        Some(SelectedHotbarSlot(1)),
    );
    assert!(
        app.world().get::<SelectedHotbarSlot>(remote).is_none(),
        "remote character's selection must remain absent after scroll",
    );
}
