//! Pure-data [`Inventory`] container — no `Component`, no events, no ECS.
//!
//! [`Inventory`] is a flat, fixed-capacity sequence of optional
//! [`ItemStack`] slots together with the mutation primitives the rest of
//! the system relies on (auto-merging insert, strict insert, take,
//! split-take, set).  It is intentionally **just data** so the same
//! container can power:
//!
//! - [`InventoryComponent`](crate::component::InventoryComponent) — an
//!   ECS component attached to an entity, where each mutation fires an
//!   [`InventoryChanged`](crate::component::InventoryChanged) event
//!   carrying the holder [`Entity`].
//! - [`BlockInventory`](crate::block::BlockInventory) — a typed
//!   [`BlockData`](dd40_core::block::BlockData) attached to a specific
//!   block cell (chests, hoppers, furnaces, ...), where each mutation
//!   fires a [`BlockInventoryChanged`](crate::block::BlockInventoryChanged)
//!   event carrying the [`BlockPos`](dd40_core::block::BlockPos) of the
//!   container block.
//!
//! Wrapper types are responsible for emitting events; [`Inventory`] only
//! reports the per-slot diff that a wrapper can package into an event.
//!
//! # Return shape
//!
//! Every mutator returns the [`Vec<SlotChange>`] it produced.  An empty
//! vector means "no observable change" — wrapper types use that to skip
//! emitting an event.
//!
//! # `slots` is private
//!
//! The slot vector is intentionally private so wrappers cannot bypass
//! the diff-reporting invariant by writing to `inv.slots[i]` through a
//! `&mut Inventory`.  Read-only access is via [`Inventory::slots`].

use std::fmt;
use std::num::NonZero;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use dd40_core::tools::ToolKindId;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::messages::ItemSelector;
use dd40_item_core::registry::{ItemId, ItemRegistry};

/// A change to a single inventory slot.
///
/// Returned by every [`Inventory`] mutator and forwarded by wrappers
/// inside `InventoryChanged` / `BlockInventoryChanged` events.  Fields
/// use [`Option<ItemStack>`] because both the previous and current
/// states may be empty (e.g. a [`take_slot`][Inventory::take_slot]
/// gives `previous = Some(_)`, `current = None`; a strict insert into
/// an empty slot gives the inverse).
#[derive(Debug, Clone, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct SlotChange {
    /// Index of the slot that changed.
    pub slot: usize,
    /// Stack that was in the slot before the call.
    pub previous: Option<ItemStack>,
    /// Stack that is in the slot after the call.
    pub current: Option<ItemStack>,
}

/// Errors returned by [`Inventory::insert_stack_strict`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertError {
    /// The supplied slot index is `>= capacity`.
    OutOfBounds {
        /// The offending slot index.
        slot: usize,
        /// The inventory's current capacity.
        capacity: usize,
    },
    /// The supplied slot is already occupied.
    SlotOccupied {
        /// The offending slot index.
        slot: usize,
    },
}

impl fmt::Display for InsertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { slot, capacity } => write!(
                f,
                "slot {slot} is out of bounds (inventory capacity {capacity})"
            ),
            Self::SlotOccupied { slot } => write!(f, "slot {slot} is already occupied"),
        }
    }
}

impl std::error::Error for InsertError {}

/// Fixed-capacity container of [`ItemStack`] slots.
///
/// Pure data: not an ECS component on its own.  Wrap in
/// [`InventoryComponent`](crate::component::InventoryComponent) to attach
/// to an entity, or in
/// [`BlockInventory`](crate::block::BlockInventory) to attach to a block
/// cell.
///
/// Use [`Inventory::with_capacity`] to construct.  The
/// [`Default`] impl yields a zero-capacity
/// inventory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    /// Creates an inventory with `capacity` empty slots.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
        }
    }

    /// Returns the number of slots in the inventory.
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Returns the stack in `slot`, or `None` if the slot is empty or out
    /// of bounds.
    pub fn slot(&self, slot: usize) -> Option<&ItemStack> {
        self.slots.get(slot).and_then(|s| s.as_ref())
    }

    /// Returns a read-only view of every slot, including empty ones.
    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    /// Iterates over every non-empty slot as `(slot_index, &stack)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &ItemStack)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| (i, s)))
    }

    /// Returns `true` when every slot is `None`.
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| s.is_none())
    }

    /// Returns `true` when no slot is `None`.
    ///
    /// Note this does not consider stack saturation: a "full" inventory
    /// by this definition may still accept more items via merging.
    pub fn is_full(&self) -> bool {
        !self.slots.is_empty() && self.slots.iter().all(|s| s.is_some())
    }

    /// Returns the total count of `item` across all slots.
    pub fn count_of(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|s| s.item == item)
            .map(|s| u32::from(s.count.get()))
            .sum()
    }

    // ─── Selector matching ───────────────────────────────────────────────

    /// Returns the index of the slot that best matches `selector`, or
    /// `None` if nothing matches.
    ///
    /// Tie-break rules:
    ///
    /// - [`ItemSelector::Exact`] — first slot (lowest index) holding the
    ///   item.
    /// - [`ItemSelector::BestToolFor`] — slot whose item's tool behaviour
    ///   matches `kind` with the highest tier; ties on tier broken by
    ///   lowest slot index.
    /// - [`ItemSelector::Placeable`] — first slot whose item's
    ///   `placeable` field equals `Some(block)`.
    pub fn find_slot(&self, selector: ItemSelector, registry: &ItemRegistry) -> Option<usize> {
        match selector {
            ItemSelector::Exact(target) => self
                .iter()
                .find(|(_, stack)| stack.item == target)
                .map(|(i, _)| i),
            ItemSelector::BestToolFor { kind } => self.find_best_tool(kind, registry),
            ItemSelector::Placeable(block) => self
                .iter()
                .find(|(_, stack)| {
                    registry.get(stack.item).and_then(|def| def.placeable) == Some(block)
                })
                .map(|(i, _)| i),
        }
    }

    fn find_best_tool(&self, kind: ToolKindId, registry: &ItemRegistry) -> Option<usize> {
        let mut best: Option<(usize, dd40_core::tools::ToolTierId)> = None;
        for (idx, stack) in self.iter() {
            let Some(def) = registry.get(stack.item) else {
                continue;
            };
            let Some(tool) = def.tool else { continue };
            if tool.kind != kind {
                continue;
            }
            match best {
                None => best = Some((idx, tool.tier)),
                Some((_, current_tier)) if tool.tier.0 > current_tier.0 => {
                    best = Some((idx, tool.tier));
                }
                _ => {}
            }
        }
        best.map(|(idx, _)| idx)
    }

    // ─── Mutators ────────────────────────────────────────────────────────

    /// Auto-merging insert.
    ///
    /// Fills existing partial stacks of the same [`ItemId`] first
    /// (capped at the item's `max_stack`), then places remaining items
    /// into empty slots.  Returns `(leftover, changes)`: the leftover
    /// stack the inventory could not absorb, plus the per-slot diff.
    ///
    /// If `stack.item` is not registered, falls back to `max_stack = 1`.
    pub fn insert_stack(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
    ) -> (Option<ItemStack>, Vec<SlotChange>) {
        let item = stack.item;
        let max_stack: u16 = registry
            .get(item)
            .map(|def| def.max_stack.get())
            .unwrap_or(1);
        let mut remaining: u16 = stack.count.get();
        let mut changes: Vec<SlotChange> = Vec::new();

        // Pass 1 — top up existing partial stacks of the same item.
        for (idx, cell) in self.slots.iter_mut().enumerate() {
            if remaining == 0 {
                break;
            }
            let Some(existing) = cell.as_mut() else {
                continue;
            };
            let existing_count = existing.count.get();
            if existing.item != item || existing_count >= max_stack {
                continue;
            }
            let previous = Some(*existing);
            let space = max_stack - existing_count;
            let moved = space.min(remaining);
            let new_count = existing_count + moved;
            existing.count = NonZero::new(new_count).expect("existing was non-zero");
            remaining -= moved;
            changes.push(SlotChange {
                slot: idx,
                previous,
                current: Some(*existing),
            });
        }

        // Pass 2 — place remainder into empty slots.
        for (idx, cell) in self.slots.iter_mut().enumerate() {
            if remaining == 0 {
                break;
            }
            if cell.is_some() {
                continue;
            }
            let take = max_stack.min(remaining);
            let take_nz = NonZero::new(take).expect("max_stack >= 1 and remaining > 0");
            let placed = ItemStack::new(item, take_nz);
            *cell = Some(placed);
            remaining -= take;
            changes.push(SlotChange {
                slot: idx,
                previous: None,
                current: Some(placed),
            });
        }

        let leftover = NonZero::new(remaining).map(|count| ItemStack { item, count });
        (leftover, changes)
    }

    /// Per-slot insert.  Fails if the slot is out of bounds or already
    /// occupied.  On success returns the single-entry diff.
    pub fn insert_stack_strict(
        &mut self,
        slot: usize,
        stack: ItemStack,
    ) -> Result<SlotChange, InsertError> {
        let capacity = self.slots.len();
        let cell = self
            .slots
            .get_mut(slot)
            .ok_or(InsertError::OutOfBounds { slot, capacity })?;
        if cell.is_some() {
            return Err(InsertError::SlotOccupied { slot });
        }
        let placed = stack;
        *cell = Some(stack);
        Ok(SlotChange {
            slot,
            previous: None,
            current: Some(placed),
        })
    }

    /// Removes and returns the entire stack in `slot`.  Returns
    /// `(None, vec![])` if the slot is empty or out of bounds; otherwise
    /// returns the taken stack together with its single-entry diff.
    pub fn take_slot(&mut self, slot: usize) -> (Option<ItemStack>, Vec<SlotChange>) {
        let previous = self.slots.get(slot).copied().flatten();
        let taken = self.slots.get_mut(slot).and_then(|s| s.take());
        if taken.is_some() {
            (
                taken,
                vec![SlotChange {
                    slot,
                    previous,
                    current: None,
                }],
            )
        } else {
            (None, Vec::new())
        }
    }

    /// Removes up to `n` items from the stack in `slot`.
    ///
    /// Returns `(None, vec![])` for no-ops (`n == 0`, empty slot, out of
    /// bounds).  Otherwise returns a stack of `min(n, count)` items
    /// together with the single-entry diff describing the slot's new
    /// state.
    pub fn take_slot_n(&mut self, slot: usize, n: u16) -> (Option<ItemStack>, Vec<SlotChange>) {
        if n == 0 {
            return (None, Vec::new());
        }
        let previous = self.slots.get(slot).copied().flatten();
        let Some(cell) = self.slots.get_mut(slot) else {
            return (None, Vec::new());
        };
        let Some(stack) = cell.as_mut() else {
            return (None, Vec::new());
        };
        let taken = if n >= stack.count.get() {
            cell.take()
        } else {
            let remaining = stack.count.get() - n;
            let taken_count = NonZero::new(n).expect("n > 0 checked above");
            let taken_stack = ItemStack::new(stack.item, taken_count);
            stack.count = NonZero::new(remaining).expect("remaining > 0 since n < count");
            Some(taken_stack)
        };
        let current = self.slots.get(slot).copied().flatten();
        (
            taken,
            vec![SlotChange {
                slot,
                previous,
                current,
            }],
        )
    }

    /// Replaces the contents of `slot` with `stack`.  Returns the
    /// previous occupant together with the single-entry diff.
    ///
    /// Out-of-bounds writes are no-ops and return `(None, vec![])`.
    /// Setting a slot to its current value is also a no-op and returns
    /// `(previous, vec![])` so callers can distinguish "nothing
    /// happened" from "the slot really is now empty".
    pub fn set_slot(
        &mut self,
        slot: usize,
        stack: Option<ItemStack>,
    ) -> (Option<ItemStack>, Vec<SlotChange>) {
        let Some(cell) = self.slots.get_mut(slot) else {
            return (None, Vec::new());
        };
        if *cell == stack {
            return (*cell, Vec::new());
        }
        let previous = *cell;
        *cell = stack;
        (
            previous,
            vec![SlotChange {
                slot,
                previous,
                current: stack,
            }],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockId;
    use dd40_core::tools::{ToolKindId, ToolTierId};
    use dd40_item_core::registry::ItemDefinition;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("nz literal must be non-zero")
    }

    fn registry_with_basics() -> ItemRegistry {
        let mut reg = ItemRegistry::new();
        reg.register(ItemDefinition::new(ItemId(1), "stone").with_max_stack(nz(64)));
        reg.register(ItemDefinition::new(ItemId(2), "tool").with_max_stack(nz(1)));
        reg.register(ItemDefinition::new(ItemId(3), "egg").with_max_stack(nz(16)));
        reg
    }

    // ─── Construction & read API ─────────────────────────────────────────

    #[test]
    fn with_capacity_yields_all_empty() {
        let inv = Inventory::with_capacity(4);
        assert_eq!(inv.capacity(), 4);
        assert!(inv.is_empty());
        assert!(!inv.is_full());
        for i in 0..4 {
            assert!(inv.slot(i).is_none());
        }
    }

    #[test]
    fn default_is_zero_capacity() {
        let inv = Inventory::default();
        assert_eq!(inv.capacity(), 0);
        assert!(inv.is_empty());
        assert!(!inv.is_full(), "zero-capacity inventory is not full");
    }

    #[test]
    fn slot_returns_none_out_of_bounds() {
        let inv = Inventory::with_capacity(2);
        assert!(inv.slot(99).is_none());
    }

    #[test]
    fn iter_skips_empties_and_yields_indices() {
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot(1, Some(ItemStack::single(ItemId(1))));
        inv.set_slot(3, Some(ItemStack::new(ItemId(2), nz(1))));
        let collected: Vec<_> = inv.iter().map(|(i, s)| (i, s.item)).collect();
        assert_eq!(collected, vec![(1, ItemId(1)), (3, ItemId(2))]);
    }

    #[test]
    fn count_of_sums_across_slots() {
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(30))));
        inv.set_slot(2, Some(ItemStack::new(ItemId(1), nz(12))));
        inv.set_slot(3, Some(ItemStack::single(ItemId(2))));
        assert_eq!(inv.count_of(ItemId(1)), 42);
        assert_eq!(inv.count_of(ItemId(2)), 1);
        assert_eq!(inv.count_of(ItemId(99)), 0);
    }

    #[test]
    fn is_full_only_when_no_none_slots() {
        let mut inv = Inventory::with_capacity(2);
        assert!(!inv.is_full());
        inv.set_slot(0, Some(ItemStack::single(ItemId(1))));
        assert!(!inv.is_full());
        inv.set_slot(1, Some(ItemStack::single(ItemId(2))));
        assert!(inv.is_full());
    }

    // ─── Mutators & diff shape ───────────────────────────────────────────

    #[test]
    fn take_slot_empties_the_slot_and_diffs_it() {
        let mut inv = Inventory::with_capacity(2);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(5))));
        let (taken, diff) = inv.take_slot(0);
        assert_eq!(taken.unwrap().count.get(), 5);
        assert!(inv.slot(0).is_none());
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].slot, 0);
        assert!(diff[0].previous.is_some());
        assert!(diff[0].current.is_none());
    }

    #[test]
    fn take_slot_oob_is_empty_diff() {
        let mut inv = Inventory::with_capacity(1);
        let (taken, diff) = inv.take_slot(99);
        assert!(taken.is_none());
        assert!(diff.is_empty());
    }

    #[test]
    fn take_slot_n_splits_when_n_lt_count() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(10))));
        let (taken, diff) = inv.take_slot_n(0, 3);
        assert_eq!(taken.unwrap().count.get(), 3);
        assert_eq!(inv.slot(0).unwrap().count.get(), 7);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].current.as_ref().unwrap().count.get(), 7);
    }

    #[test]
    fn take_slot_n_clears_when_n_ge_count() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(4))));
        let (taken, diff) = inv.take_slot_n(0, 99);
        assert_eq!(taken.unwrap().count.get(), 4);
        assert!(inv.slot(0).is_none());
        assert_eq!(diff.len(), 1);
        assert!(diff[0].current.is_none());
    }

    #[test]
    fn take_slot_n_zero_is_empty_diff() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(5))));
        let (taken, diff) = inv.take_slot_n(0, 0);
        assert!(taken.is_none());
        assert!(diff.is_empty());
        assert_eq!(inv.slot(0).unwrap().count.get(), 5);
    }

    #[test]
    fn set_slot_returns_previous_and_diff() {
        let mut inv = Inventory::with_capacity(1);
        let (prev, diff) = inv.set_slot(0, Some(ItemStack::single(ItemId(1))));
        assert!(prev.is_none());
        assert_eq!(diff.len(), 1);
        let (prev2, diff2) = inv.set_slot(0, Some(ItemStack::single(ItemId(2))));
        assert_eq!(prev2.unwrap().item, ItemId(1));
        assert_eq!(diff2.len(), 1);
    }

    #[test]
    fn set_slot_to_identical_is_empty_diff() {
        let mut inv = Inventory::with_capacity(1);
        let stack = ItemStack::new(ItemId(1), nz(3));
        inv.set_slot(0, Some(stack));
        let (prev, diff) = inv.set_slot(0, Some(stack));
        assert_eq!(prev.unwrap(), stack);
        assert!(diff.is_empty());
    }

    #[test]
    fn set_slot_oob_is_empty_diff() {
        let mut inv = Inventory::with_capacity(1);
        let (prev, diff) = inv.set_slot(99, Some(ItemStack::single(ItemId(1))));
        assert!(prev.is_none());
        assert!(diff.is_empty());
    }

    #[test]
    fn insert_stack_into_empty_uses_first_slot() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        let (leftover, diff) = inv.insert_stack(ItemStack::new(ItemId(1), nz(5)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 5);
        assert_eq!(diff.len(), 1);
    }

    #[test]
    fn insert_stack_merges_into_partial() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(60))));
        let (leftover, diff) = inv.insert_stack(ItemStack::new(ItemId(1), nz(3)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 63);
        assert_eq!(diff.len(), 1);
    }

    #[test]
    fn insert_stack_overflows_into_next_empty() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(60))));
        let (leftover, diff) = inv.insert_stack(ItemStack::new(ItemId(1), nz(10)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 64);
        assert_eq!(inv.slot(1).unwrap().count.get(), 6);
        assert_eq!(diff.len(), 2);
    }

    #[test]
    fn insert_stack_returns_leftover_when_full() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot(0, Some(ItemStack::new(ItemId(1), nz(64))));
        let (leftover, diff) = inv.insert_stack(ItemStack::new(ItemId(1), nz(5)), &reg);
        assert_eq!(leftover.unwrap().count.get(), 5);
        assert_eq!(inv.slot(0).unwrap().count.get(), 64);
        assert!(diff.is_empty());
    }

    #[test]
    fn insert_stack_non_stackable_one_per_slot() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        let (leftover, diff) = inv.insert_stack(ItemStack::new(ItemId(2), nz(3)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 1);
        assert_eq!(inv.slot(1).unwrap().count.get(), 1);
        assert_eq!(inv.slot(2).unwrap().count.get(), 1);
        assert_eq!(diff.len(), 3);
    }

    #[test]
    fn insert_strict_success() {
        let mut inv = Inventory::with_capacity(2);
        let change = inv
            .insert_stack_strict(1, ItemStack::single(ItemId(1)))
            .expect("ok");
        assert_eq!(change.slot, 1);
        assert_eq!(inv.slot(1).unwrap().item, ItemId(1));
    }

    #[test]
    fn insert_strict_slot_occupied() {
        let mut inv = Inventory::with_capacity(2);
        inv.set_slot(0, Some(ItemStack::single(ItemId(1))));
        let err = inv
            .insert_stack_strict(0, ItemStack::single(ItemId(2)))
            .unwrap_err();
        assert_eq!(err, InsertError::SlotOccupied { slot: 0 });
    }

    #[test]
    fn insert_strict_out_of_bounds() {
        let mut inv = Inventory::with_capacity(2);
        let err = inv
            .insert_stack_strict(7, ItemStack::single(ItemId(1)))
            .unwrap_err();
        assert_eq!(
            err,
            InsertError::OutOfBounds {
                slot: 7,
                capacity: 2
            }
        );
    }

    // ─── ItemSelector matching ──────────────────────────────────────────

    fn registry_with_tools() -> ItemRegistry {
        let mut reg = ItemRegistry::new();
        let pickaxe = ToolKindId(1);
        reg.register(
            ItemDefinition::new(ItemId(10), "wood_pick")
                .with_max_stack(nz(1))
                .with_tool(pickaxe, ToolTierId(1)),
        );
        reg.register(
            ItemDefinition::new(ItemId(11), "iron_pick")
                .with_max_stack(nz(1))
                .with_tool(pickaxe, ToolTierId(3)),
        );
        reg.register(
            ItemDefinition::new(ItemId(12), "axe")
                .with_max_stack(nz(1))
                .with_tool(ToolKindId(2), ToolTierId(2)),
        );
        reg.register(
            ItemDefinition::new(ItemId(20), "dirt")
                .with_max_stack(nz(64))
                .with_placeable(BlockId(7)),
        );
        reg
    }

    #[test]
    fn find_slot_exact_returns_first_match() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot(2, Some(ItemStack::single(ItemId(11))));
        inv.set_slot(3, Some(ItemStack::single(ItemId(11))));
        let hit = inv.find_slot(ItemSelector::Exact(ItemId(11)), &reg);
        assert_eq!(hit, Some(2));
    }

    #[test]
    fn find_slot_best_tool_picks_highest_tier() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot(0, Some(ItemStack::single(ItemId(10))));
        inv.set_slot(2, Some(ItemStack::single(ItemId(11))));
        let hit = inv.find_slot(
            ItemSelector::BestToolFor {
                kind: ToolKindId(1),
            },
            &reg,
        );
        assert_eq!(hit, Some(2));
    }

    #[test]
    fn find_slot_best_tool_no_kind_match_is_none() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(2);
        inv.set_slot(0, Some(ItemStack::single(ItemId(12))));
        let hit = inv.find_slot(
            ItemSelector::BestToolFor {
                kind: ToolKindId(1),
            },
            &reg,
        );
        assert_eq!(hit, None);
    }

    #[test]
    fn find_slot_placeable_first_match() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot(1, Some(ItemStack::new(ItemId(20), nz(8))));
        let hit = inv.find_slot(ItemSelector::Placeable(BlockId(7)), &reg);
        assert_eq!(hit, Some(1));
    }

    #[test]
    fn find_slot_placeable_no_match() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot(0, Some(ItemStack::single(ItemId(10))));
        let hit = inv.find_slot(ItemSelector::Placeable(BlockId(99)), &reg);
        assert_eq!(hit, None);
    }
}
