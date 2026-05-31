//! Apply layer: drains [`SlotInteraction`] messages and applies the
//! per-intent [`rules`][crate::rules] resolvers to the target
//! character's [`InventoryComponent`] and per-character
//! [`HeldStackComponent`].
//!
//! This system is the only place outside the pure resolvers where the
//! cursor and an `Inventory` mutate together.  Quick-transfer
//! (cross-section move) and drop-held (drop-into-world) are both
//! handled directly here because they need information beyond a
//! single slot.

use std::num::NonZero;

use bevy::prelude::*;
use dd40_inventory_core::prelude::{
    DropItems, HOTBAR_SIZE, HeldStackComponent, InventoryComponent, SlotInteraction,
    SlotInteractionKind,
};
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;

use crate::rules::{
    SlotResolution, resolve_place_all, resolve_place_one, resolve_take_all, resolve_take_half,
};

/// Drains [`SlotInteraction`] messages and mutates the targeted
/// inventory + that character's [`HeldStackComponent`] accordingly.
///
/// `DropHeld` emits a [`DropItems`] message at the character's
/// `Transform.translation` and clears the cursor.  In v1 there is no
/// scatter velocity — drops appear at the player's feet.
pub fn apply_slot_interactions(
    mut reader: MessageReader<SlotInteraction>,
    mut commands: Commands,
    mut inventories: Query<(&mut InventoryComponent, &mut HeldStackComponent, &Transform)>,
    registry: Res<ItemRegistry>,
    mut drops: MessageWriter<DropItems>,
) {
    for msg in reader.read() {
        let Ok((mut inv_comp, mut held, transform)) = inventories.get_mut(msg.character) else {
            warn!(
                "SlotInteraction for unknown character entity {:?}",
                msg.character
            );
            continue;
        };
        match &msg.kind {
            SlotInteractionKind::DropHeld => {
                let Some(stack) = held.take() else {
                    continue;
                };
                drops.write(DropItems {
                    origin: transform.translation,
                    velocity: Vec3::ZERO,
                    stacks: vec![stack],
                });
            }
            SlotInteractionKind::QuickTransfer { slot } => {
                let slot_idx = *slot as usize;
                if !check_slot_bounds(slot_idx, inv_comp.inventory().capacity()) {
                    continue;
                }
                quick_transfer(
                    &mut inv_comp,
                    slot_idx,
                    &registry,
                    &mut commands,
                    msg.character,
                );
            }
            SlotInteractionKind::TakeAll { slot }
            | SlotInteractionKind::PlaceAll { slot }
            | SlotInteractionKind::TakeHalf { slot }
            | SlotInteractionKind::PlaceOne { slot } => {
                let slot_idx = *slot as usize;
                if !check_slot_bounds(slot_idx, inv_comp.inventory().capacity()) {
                    continue;
                }
                let current_slot = inv_comp.inventory().slot(slot_idx).copied();
                let max_stack = max_stack_for(current_slot, held.0, &registry);
                let resolution = match &msg.kind {
                    SlotInteractionKind::TakeAll { .. } => resolve_take_all(held.0, current_slot),
                    SlotInteractionKind::PlaceAll { .. } => {
                        resolve_place_all(held.0, current_slot, max_stack)
                    }
                    SlotInteractionKind::TakeHalf { .. } => resolve_take_half(held.0, current_slot),
                    SlotInteractionKind::PlaceOne { .. } => {
                        resolve_place_one(held.0, current_slot, max_stack)
                    }
                    _ => unreachable!("filtered by outer match"),
                };
                match resolution {
                    SlotResolution::NoOp => {}
                    SlotResolution::Mutation { new_held, new_slot } => {
                        inv_comp.set_slot(slot_idx, new_slot, &mut commands, msg.character);
                        held.0 = new_held;
                    }
                }
            }
        }
    }
}

fn check_slot_bounds(slot_idx: usize, capacity: usize) -> bool {
    if slot_idx >= capacity {
        warn!(
            "SlotInteraction targets out-of-bounds slot {} (capacity {})",
            slot_idx, capacity
        );
        return false;
    }
    true
}

/// Looks up the relevant `max_stack` cap for the resolver call.
///
/// The cap is only consulted when merging same-item stacks; for swaps
/// it is irrelevant.  Prefer the slot's item id since that is the
/// side the resolver writes to.
fn max_stack_for(
    slot: Option<ItemStack>,
    held: Option<ItemStack>,
    registry: &ItemRegistry,
) -> NonZero<u16> {
    let item_id = slot.map(|s| s.item).or_else(|| held.map(|h| h.item));
    item_id
        .and_then(|id| registry.get(id).map(|def| def.max_stack))
        .unwrap_or_else(|| NonZero::new(64).expect("64 nz"))
}

/// Moves the entire stack at `from_slot` to the opposite inventory
/// area (hotbar ↔ main).  Merges into matching stacks first (slot
/// order), then fills the first empty slot, and finally puts any
/// leftover back into the source slot.
fn quick_transfer(
    inv_comp: &mut InventoryComponent,
    from_slot: usize,
    registry: &ItemRegistry,
    commands: &mut Commands,
    entity: Entity,
) {
    let inv = inv_comp.inventory();
    let capacity = inv.capacity();
    if from_slot >= capacity {
        return;
    }
    let Some(source) = inv.slot(from_slot).copied() else {
        return;
    };
    let mut remaining = source;
    let in_hotbar = from_slot < HOTBAR_SIZE as usize;
    let (range_start, range_end) = if in_hotbar {
        (HOTBAR_SIZE as usize, capacity)
    } else {
        (0, HOTBAR_SIZE as usize)
    };
    let max_stack = registry
        .get(remaining.item)
        .map(|def| def.max_stack.get())
        .unwrap_or(64);

    // Empty the source first so we never merge with ourself.
    inv_comp.take_slot(from_slot, commands, entity);

    // Pass 1: merge into matching stacks with room.
    for target in range_start..range_end {
        let Some(existing) = inv_comp.inventory().slot(target).copied() else {
            continue;
        };
        if existing.item != remaining.item || existing.count.get() >= max_stack {
            continue;
        }
        let space = max_stack - existing.count.get();
        let move_amt = space.min(remaining.count.get());
        let new_existing = ItemStack::new(
            existing.item,
            NonZero::new(existing.count.get() + move_amt).expect("> 0"),
        );
        inv_comp.set_slot(target, Some(new_existing), commands, entity);
        let leftover = remaining.count.get() - move_amt;
        match NonZero::new(leftover) {
            Some(c) => remaining = ItemStack::new(remaining.item, c),
            None => return,
        }
    }

    // Pass 2: first empty slot in range.
    for target in range_start..range_end {
        if inv_comp.inventory().slot(target).is_some() {
            continue;
        }
        inv_comp.set_slot(target, Some(remaining), commands, entity);
        return;
    }

    // No room — return leftover to source.
    inv_comp.set_slot(from_slot, Some(remaining), commands, entity);
}
