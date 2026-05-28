//! Hotbar slot selection and `ActiveItem` sync.
//!
//! This module owns the policy that turns local input (number keys, mouse
//! wheel) and external [`RequestActiveItem`] messages into changes of the
//! local character's [`SelectedHotbarSlot`], and projects the slot's
//! contents onto [`ActiveItem`] so the rest of the game (mining, placement,
//! HUD) can read it without knowing anything about inventories.
//!
//! ## Single-player assumption
//!
//! v1 of the inventory is **local-only** and assumes there is at most one
//! [`Player`] entity (the locally-controlled character) at any time.
//! Number-key and wheel inputs are routed to the unique `Player`; if more
//! than one is present (split-screen, debug spawns), the first one yielded
//! by the query wins.  Multi-character work will need an explicit
//! `Action<HotbarSelect>` → context-entity lookup instead.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, ActionEvents};
use dd40_character_core::components::Player;
use dd40_input_core::actions::HotbarSelect;
use dd40_inventory_core::prelude::{
    HOTBAR_SIZE, InventoryChanged, InventoryComponent, SelectedHotbarSlot,
};
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::messages::{ItemSelector, RequestActiveItem};
use dd40_item_core::registry::ItemRegistry;

/// Ensures every [`Player`] has a [`SelectedHotbarSlot`] component.
///
/// The selection systems assume the component exists; this saves every
/// spawn site (network, debug) from having to remember to add it.
pub fn ensure_selected_slot(
    mut commands: Commands,
    players: Query<Entity, (Added<Player>, Without<SelectedHotbarSlot>)>,
) {
    for player in &players {
        commands
            .entity(player)
            .insert(SelectedHotbarSlot::default());
    }
}

/// Reads the press edge of [`HotbarSelect`] and writes the new slot to the
/// local [`Player`].  The action's `f32` value is the 1-based slot index;
/// values outside `1.0..=HOTBAR_SIZE` are ignored.
pub fn apply_hotbar_keys(
    actions: Query<(&Action<HotbarSelect>, &ActionEvents)>,
    mut players: Query<&mut SelectedHotbarSlot, With<Player>>,
) {
    let Ok(mut slot) = players.single_mut() else {
        return;
    };
    for (action, events) in &actions {
        if !events.contains(ActionEvents::START) {
            continue;
        }
        let value: f32 = **action;
        let one_based = value.round() as i32;
        if (1..=HOTBAR_SIZE as i32).contains(&one_based) {
            slot.set_wrapped((one_based - 1) as i16);
        }
    }
}

/// Reads mouse-wheel scroll events and shifts the local player's selected
/// hotbar slot.  Scrolling up (positive `y`) moves left (toward slot 0)
/// to match the convention of most block games.
pub fn apply_hotbar_wheel(
    mut wheel: MessageReader<MouseWheel>,
    mut players: Query<&mut SelectedHotbarSlot, With<Player>>,
) {
    let Ok(mut slot) = players.single_mut() else {
        return;
    };
    let mut delta: i32 = 0;
    for ev in wheel.read() {
        let steps = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 30.0,
        };
        delta -= steps.round() as i32;
    }
    if delta != 0 {
        slot.shift(delta as i16);
    }
}

/// Drains [`RequestActiveItem`] messages and updates the player's selected
/// hotbar slot to one matching the request, when possible.  Silently
/// drops requests with no matching slot.
pub fn apply_active_item_requests(
    mut reader: MessageReader<RequestActiveItem>,
    registry: Res<ItemRegistry>,
    mut characters: Query<(&InventoryComponent, &mut SelectedHotbarSlot)>,
) {
    for msg in reader.read() {
        let Ok((inv_comp, mut slot)) = characters.get_mut(msg.entity) else {
            continue;
        };
        if let Some(found) = find_slot_for(inv_comp.inventory(), msg.selector, &registry) {
            slot.set_wrapped(found as i16);
        }
    }
}

fn find_slot_for(
    inv: &dd40_inventory_core::inventory::Inventory,
    selector: ItemSelector,
    registry: &ItemRegistry,
) -> Option<u8> {
    let limit = HOTBAR_SIZE as usize;
    match selector {
        ItemSelector::Exact(target) => {
            (0..limit).find_map(|i| inv.slot(i).filter(|s| s.item == target).map(|_| i as u8))
        }
        ItemSelector::Placeable(block) => (0..limit).find_map(|i| {
            let stack = inv.slot(i)?;
            let def = registry.get(stack.item)?;
            (def.placeable == Some(block)).then_some(i as u8)
        }),
        ItemSelector::BestToolFor { kind } => {
            let mut best: Option<(u8, u16)> = None;
            for i in 0..limit {
                let Some(stack) = inv.slot(i) else { continue };
                let Some(def) = registry.get(stack.item) else {
                    continue;
                };
                let Some(tool) = def.tool else { continue };
                if tool.kind != kind {
                    continue;
                }
                let tier = tool.tier.0;
                if best.is_none_or(|(_, t)| tier > t) {
                    best = Some((i as u8, tier));
                }
            }
            best.map(|(slot, _)| slot)
        }
    }
}

/// Recomputes [`ActiveItem`] from the selected slot whenever the slot
/// changes.  Runs in [`Update`] alongside the input systems so the new
/// active item is visible to mining/placement on the next tick.
pub fn sync_active_item_on_slot_change(
    mut characters: Query<
        (&InventoryComponent, &SelectedHotbarSlot, &mut ActiveItem),
        Changed<SelectedHotbarSlot>,
    >,
) {
    for (inv_comp, slot, mut active) in &mut characters {
        let new = inv_comp.inventory().slot(slot.0 as usize).copied();
        if active.0 != new {
            active.0 = new;
        }
    }
}

/// Observer: when an inventory changes, refresh [`ActiveItem`] if any
/// changed slot is the currently-selected hotbar slot.
pub fn sync_active_item_on_inventory_change(
    trigger: On<InventoryChanged>,
    mut characters: Query<(&InventoryComponent, &SelectedHotbarSlot, &mut ActiveItem)>,
) {
    let Ok((inv_comp, slot, mut active)) = characters.get_mut(trigger.entity) else {
        return;
    };
    let selected = slot.0 as usize;
    if !trigger.changes.iter().any(|change| change.slot == selected) {
        return;
    }
    let new = inv_comp.inventory().slot(selected).copied();
    if active.0 != new {
        active.0 = new;
    }
}

/// Ensures every [`Player`] also has an [`ActiveItem`] component so the
/// sync systems have somewhere to write.  Mining/placement systems treat
/// the absence of the component as "bare hands"; we make it explicit so
/// observers don't have to insert it.
pub fn ensure_active_item(
    mut commands: Commands,
    players: Query<Entity, (Added<Player>, Without<ActiveItem>)>,
) {
    for player in &players {
        commands.entity(player).insert(ActiveItem::default());
    }
}
