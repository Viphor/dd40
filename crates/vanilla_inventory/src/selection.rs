//! Hotbar input → `SetActiveSlot` messages, plus the apply system that
//! consumes them.
//!
//! This module owns the policy that turns local input (number keys, mouse
//! wheel) and external [`RequestActiveItem`] messages into
//! [`SetActiveSlot`] requests targeting the local [`Player`].  The
//! authoritative side (server in networked builds, or the same process
//! in single-player) drains the requests in [`apply_set_active_slot`]
//! and calls [`InventoryComponent::set_active_slot`], which propagates
//! back to clients via replication.
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
use dd40_inventory_core::prelude::{HOTBAR_SIZE, InventoryComponent, SetActiveSlot};
use dd40_item_core::messages::{ItemSelector, RequestActiveItem};
use dd40_item_core::registry::ItemRegistry;

/// Reads the press edge of [`HotbarSelect`] and emits a
/// [`SetActiveSlot`] for the local [`Player`].  The action's `f32`
/// value is the 1-based slot index; values outside `1.0..=HOTBAR_SIZE`
/// are ignored.
pub fn apply_hotbar_keys(
    actions: Query<(&Action<HotbarSelect>, &ActionEvents)>,
    players: Query<Entity, With<Player>>,
    mut writer: MessageWriter<SetActiveSlot>,
) {
    let Ok(player) = players.single() else {
        return;
    };
    for (action, events) in &actions {
        if !events.contains(ActionEvents::START) {
            continue;
        }
        let value: f32 = **action;
        let one_based = value.round() as i32;
        if (1..=HOTBAR_SIZE as i32).contains(&one_based) {
            writer.write(SetActiveSlot {
                character: player,
                slot: (one_based - 1) as u8,
            });
        }
    }
}

/// Reads mouse-wheel scroll events and shifts the local player's active
/// hotbar slot by emitting a wrapped [`SetActiveSlot`].  Scrolling up
/// (positive `y`) moves left (toward slot 0) to match the convention
/// of most block games.
pub fn apply_hotbar_wheel(
    mut wheel: MessageReader<MouseWheel>,
    players: Query<(Entity, &InventoryComponent), With<Player>>,
    mut writer: MessageWriter<SetActiveSlot>,
) {
    let Ok((player, inv)) = players.single() else {
        wheel.read().for_each(|_| {});
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
    if delta == 0 {
        return;
    }
    let size = HOTBAR_SIZE as i32;
    let current = inv.inventory().active_slot() as i32;
    let next = (current + delta).rem_euclid(size) as u8;
    writer.write(SetActiveSlot {
        character: player,
        slot: next,
    });
}

/// Drains [`RequestActiveItem`] messages and emits a [`SetActiveSlot`]
/// targeting the slot in the recipient's inventory that best matches
/// the request.  Silently drops requests with no matching slot.
pub fn apply_active_item_requests(
    mut reader: MessageReader<RequestActiveItem>,
    registry: Res<ItemRegistry>,
    characters: Query<&InventoryComponent>,
    mut writer: MessageWriter<SetActiveSlot>,
) {
    for msg in reader.read() {
        let Ok(inv_comp) = characters.get(msg.entity) else {
            continue;
        };
        if let Some(found) = find_slot_for(inv_comp.inventory(), msg.selector, &registry) {
            writer.write(SetActiveSlot {
                character: msg.entity,
                slot: found,
            });
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

/// Authoritative apply system: drains [`SetActiveSlot`] and writes
/// the resulting index onto each target character's
/// [`InventoryComponent`].
///
/// Lives in the rules half of `dd40_vanilla_inventory` (server-side
/// in networked builds) so client-emitted requests round-trip through
/// the server and the new value comes back via `InventoryComponent`
/// replication — the same shape as every other inventory mutation.
pub fn apply_set_active_slot(
    mut reader: MessageReader<SetActiveSlot>,
    mut inventories: Query<&mut InventoryComponent>,
) {
    for msg in reader.read() {
        let Ok(mut inv) = inventories.get_mut(msg.character) else {
            warn!(
                "SetActiveSlot for unknown character entity {:?}",
                msg.character
            );
            continue;
        };
        inv.set_active_slot(msg.slot);
    }
}
