//! The [`Inventory`] component, the [`InventoryChanged`] event, and the
//! mutating API.
//!
//! # Mutator pattern
//!
//! Every mutating method comes in two flavours:
//!
//! - **Event-firing** (the recommended path) — takes
//!   `&mut Commands` and the holder [`Entity`] and triggers an
//!   [`InventoryChanged`] event after the mutation.
//! - **Silent** (`*_without_event`) — performs the same mutation without
//!   touching `Commands`.  Intended for tests, pre-spawn population, and
//!   batch operations where the caller wants to fire one summary event.
//!
//! This mirrors the
//! [`BlockRegistry::register`][dd40_core::block::registry::BlockRegistry::register] /
//! [`register_without_event`][dd40_core::block::registry::BlockRegistry::register_without_event]
//! precedent in `dd40_core`.
//!
//! # `slots` is private
//!
//! The slot vector is intentionally private so the event-firing invariant
//! cannot be bypassed by writing to `inv.slots[i]` directly through a
//! `&mut Inventory`.  Read-only access is via [`Inventory::slots`].

use std::fmt;
use std::num::NonZero;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use dd40_core::tools::ToolKindId;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::messages::ItemSelector;
use dd40_item_core::registry::{ItemId, ItemRegistry};

/// A change to a single inventory slot, carried in [`InventoryChanged`].
///
/// Fields use [`Option<ItemStack>`] because both the previous and current
/// states may be empty (e.g. a [`take_slot`][Inventory::take_slot] gives
/// `previous = Some(_)`, `current = None`; a strict insert into an empty
/// slot gives the inverse).
/// `SlotChange` derives [`PartialEq`] / [`Eq`] across all fields, but
/// implements [`PartialOrd`] / [`Ord`] **keyed only on
/// [`slot`][Self::slot]** — sorting a `Vec<SlotChange>` therefore yields
/// changes in ascending slot order regardless of stack contents.
#[derive(Debug, Clone, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct SlotChange {
    /// Index of the slot that changed.
    pub slot: usize,
    /// Stack that was in the slot before the call.
    pub previous: Option<ItemStack>,
    /// Stack that is in the slot after the call.
    pub current: Option<ItemStack>,
}

impl PartialOrd for SlotChange {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SlotChange {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.slot.cmp(&other.slot)
    }
}

/// Entity-targeted event triggered after every successful mutation of an
/// [`Inventory`].
///
/// Triggered via
/// [`Commands::trigger_targets`][bevy::prelude::Commands::trigger_targets]
/// so observers reach the holder via `trigger.target()`.
///
/// # Batching and ordering
///
/// `changes` contains exactly **one entry per slot the call modified** —
/// duplicate slot entries never appear.  An
/// [`insert_stack`][Inventory::insert_stack] that fills three slots
/// produces one event with three entries — never three separate events.
///
/// The order of entries within `changes` is **unspecified**.  Callers
/// that need ordered output should sort the vector (e.g. by
/// [`SlotChange::slot`]); [`SlotChange`] derives [`Ord`] for this reason.
///
/// No-op calls (failed strict insert, take from an empty slot, etc.) fire
/// **no** event.  "Event observed" is therefore a reliable signal that
/// inventory contents actually moved.
#[derive(EntityEvent, Debug, Clone)]
pub struct InventoryChanged {
    /// The inventory entity this event targets.  Set automatically by
    /// `EntityEvent`'s field-name convention.
    pub entity: Entity,
    /// Per-slot diff for the call that triggered this event.
    pub changes: Vec<SlotChange>,
}

/// Errors returned by [`Inventory::insert_stack_strict`] and its silent
/// counterpart.
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

/// Fixed-capacity container of [`ItemStack`] slots attached to a holder
/// entity.
///
/// Use [`Inventory::with_capacity`] to construct.  The
/// [`Default`][std::default::Default] impl yields a zero-capacity
/// inventory and exists only so the component can be registered for
/// reflection — real use always specifies a capacity.
///
/// See the [module-level documentation][self] for the mutator pattern.
#[derive(Component, Debug, Clone, Default, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
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
    /// Note this does not consider stack saturation: a "full" inventory by
    /// this definition may still accept more items via merging.
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
    /// - [`ItemSelector::BestToolFor`] — slot whose item's
    ///   [`ToolBehavior`][dd40_item_core::registry::ToolBehavior] matches
    ///   `kind` with the **highest**
    ///   [`ToolTierId`][dd40_core::tools::ToolTierId].  Ties on tier are
    ///   broken by the lowest slot index.  Items not registered in
    ///   `registry` are ignored.
    /// - [`ItemSelector::Placeable`] — first slot whose item's `placeable`
    ///   field equals `Some(block)`.
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

    // ─── Silent mutators (no event) ──────────────────────────────────────

    /// Auto-merging insert with no event emission.  Returns the leftover
    /// stack if the inventory could not absorb everything.
    ///
    /// Fills existing partial stacks of the same [`ItemId`] first (capped
    /// at [`ItemDefinition::max_stack`][dd40_item_core::registry::ItemDefinition::max_stack]),
    /// then places remaining items into empty slots.
    ///
    /// If `stack.item` is not registered, falls back to `max_stack = 1` so
    /// unknown items go into one slot per item rather than overflowing.
    ///
    /// See [`Inventory::insert_stack`] for the event-firing variant.
    pub fn insert_stack_without_event(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
    ) -> Option<ItemStack> {
        let (leftover, _changes) = self.insert_stack_inner(stack, registry);
        leftover
    }

    /// Per-slot insert with no event emission.
    ///
    /// See [`Inventory::insert_stack_strict`] for the event-firing variant.
    pub fn insert_stack_strict_without_event(
        &mut self,
        slot: usize,
        stack: ItemStack,
    ) -> Result<(), InsertError> {
        self.insert_stack_strict_inner(slot, stack).map(|_| ())
    }

    /// Removes and returns the entire stack in `slot` with no event
    /// emission.  Returns `None` if the slot is empty or out of bounds.
    ///
    /// See [`Inventory::take_slot`] for the event-firing variant.
    pub fn take_slot_without_event(&mut self, slot: usize) -> Option<ItemStack> {
        self.slots.get_mut(slot).and_then(|s| s.take())
    }

    /// Removes up to `n` items from the stack in `slot` with no event
    /// emission.  Returns `None` if the slot is empty, out of bounds, or
    /// `n == 0`; otherwise returns a stack of `min(n, count)` items and
    /// leaves the remainder in place (the slot becomes empty when fully
    /// drained).
    ///
    /// See [`Inventory::take_slot_n`] for the event-firing variant.
    pub fn take_slot_n_without_event(&mut self, slot: usize, n: u16) -> Option<ItemStack> {
        if n == 0 {
            return None;
        }
        let cell = self.slots.get_mut(slot)?;
        let stack = cell.as_mut()?;
        if n >= stack.count.get() {
            cell.take()
        } else {
            let remaining = stack.count.get() - n;
            let taken_count = NonZero::new(n).expect("n > 0 checked above");
            let taken = ItemStack::new(stack.item, taken_count);
            stack.count = NonZero::new(remaining).expect("remaining > 0 since n < count");
            Some(taken)
        }
    }

    /// Replaces the contents of `slot` with `stack` and returns the
    /// previous occupant.  No event emission.
    ///
    /// Out-of-bounds writes are silently ignored and return `None`; this
    /// matches the existing [`take_slot_without_event`][Self::take_slot_without_event]
    /// shape.
    ///
    /// See [`Inventory::set_slot`] for the event-firing variant.
    pub fn set_slot_without_event(
        &mut self,
        slot: usize,
        stack: Option<ItemStack>,
    ) -> Option<ItemStack> {
        let cell = self.slots.get_mut(slot)?;
        std::mem::replace(cell, stack)
    }

    // ─── Event-firing mutators ───────────────────────────────────────────

    /// Auto-merging insert that fires an [`InventoryChanged`] event on
    /// `entity` describing every slot it modified.
    ///
    /// Returns leftover (same as [`insert_stack_without_event`][Self::insert_stack_without_event]).
    /// No event fires when the call is a no-op (stack absorbed nothing,
    /// e.g. inventory full and no merge possible).
    pub fn insert_stack(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (leftover, changes) = self.insert_stack_inner(stack, registry);
        emit_if_nonempty(commands, entity, changes);
        leftover
    }

    /// Per-slot insert that fires an [`InventoryChanged`] event on
    /// `entity` carrying a single [`SlotChange`] on success.  Fires no
    /// event on error.
    pub fn insert_stack_strict(
        &mut self,
        slot: usize,
        stack: ItemStack,
        commands: &mut Commands,
        entity: Entity,
    ) -> Result<(), InsertError> {
        let change = self.insert_stack_strict_inner(slot, stack)?;
        emit_if_nonempty(commands, entity, vec![change]);
        Ok(())
    }

    /// Removes the stack in `slot`, firing an [`InventoryChanged`] event
    /// on `entity` when something was actually removed.  Empty / out of
    /// bounds slots are no-ops and fire no event.
    pub fn take_slot(
        &mut self,
        slot: usize,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let previous = self.slots.get(slot).and_then(|s| s.clone());
        let taken = self.take_slot_without_event(slot);
        if taken.is_some() {
            commands.trigger(InventoryChanged {
                entity,
                changes: vec![SlotChange {
                    slot,
                    previous,
                    current: None,
                }],
            });
        }
        taken
    }

    /// Removes up to `n` items from `slot`, firing an [`InventoryChanged`]
    /// event on `entity` when something was actually removed.  No event
    /// fires when the call is a no-op (`n == 0`, empty slot, out of
    /// bounds).
    pub fn take_slot_n(
        &mut self,
        slot: usize,
        n: u16,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let previous = self.slots.get(slot).and_then(|s| s.clone());
        let taken = self.take_slot_n_without_event(slot, n);
        if taken.is_some() {
            let current = self.slots.get(slot).and_then(|s| s.clone());
            commands.trigger(InventoryChanged {
                entity,
                changes: vec![SlotChange {
                    slot,
                    previous,
                    current,
                }],
            });
        }
        taken
    }

    /// Replaces the contents of `slot`, firing an [`InventoryChanged`]
    /// event on `entity` if the contents actually changed.  Out-of-bounds
    /// calls are no-ops and fire no event.  A call that replaces a slot
    /// with an identical stack fires no event.
    pub fn set_slot(
        &mut self,
        slot: usize,
        stack: Option<ItemStack>,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        // Out of bounds → no-op.
        if slot >= self.slots.len() {
            return None;
        }
        let previous = self.slots.get(slot).cloned().flatten();
        if previous == stack {
            // Identical contents — return the previous value but emit no
            // event since nothing observable changed.
            return previous;
        }
        let current_clone = stack.clone();
        let returned = self.set_slot_without_event(slot, stack);
        commands.trigger(InventoryChanged {
            entity,
            changes: vec![SlotChange {
                slot,
                previous,
                current: current_clone,
            }],
        });
        returned
    }

    // ─── Internal shared implementation ──────────────────────────────────

    fn insert_stack_inner(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
    ) -> (Option<ItemStack>, Vec<SlotChange>) {
        let item = stack.item;
        let max_stack: u16 = registry
            .get(item)
            .map(|def| def.max_stack.get())
            .unwrap_or(1);
        // Working count is platform-`u16` so it can reach zero as we drain;
        // `ItemStack::count` itself stays `NonZero<u16>`.
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

    fn insert_stack_strict_inner(
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
        let placed = stack.clone();
        *cell = Some(stack);
        Ok(SlotChange {
            slot,
            previous: None,
            current: Some(placed),
        })
    }
}

fn emit_if_nonempty(commands: &mut Commands, entity: Entity, changes: Vec<SlotChange>) {
    if changes.is_empty() {
        return;
    }
    commands.trigger(InventoryChanged { entity, changes });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use dd40_core::block::BlockId;
    use dd40_core::tools::{ToolKindId, ToolTierId};
    use dd40_item_core::registry::ItemDefinition;

    /// Test helper — wraps a literal in [`NonZero<u16>`].
    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("nz literal must be non-zero")
    }

    // ─── Helpers ─────────────────────────────────────────────────────────

    fn registry_with_basics() -> ItemRegistry {
        let mut reg = ItemRegistry::new();
        // ItemId(1): stackable resource, max 64
        reg.register(ItemDefinition::new(ItemId(1), "stone").with_max_stack(nz(64)));
        // ItemId(2): non-stackable, max 1
        reg.register(ItemDefinition::new(ItemId(2), "tool").with_max_stack(nz(1)));
        // ItemId(3): stackable to 16
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
        inv.set_slot_without_event(1, Some(ItemStack::single(ItemId(1))));
        inv.set_slot_without_event(3, Some(ItemStack::new(ItemId(2), nz(1))));
        let collected: Vec<_> = inv.iter().map(|(i, s)| (i, s.item)).collect();
        assert_eq!(collected, vec![(1, ItemId(1)), (3, ItemId(2))]);
    }

    #[test]
    fn count_of_sums_across_slots() {
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(30))));
        inv.set_slot_without_event(2, Some(ItemStack::new(ItemId(1), nz(12))));
        inv.set_slot_without_event(3, Some(ItemStack::single(ItemId(2))));
        assert_eq!(inv.count_of(ItemId(1)), 42);
        assert_eq!(inv.count_of(ItemId(2)), 1);
        assert_eq!(inv.count_of(ItemId(99)), 0);
    }

    #[test]
    fn is_full_only_when_no_none_slots() {
        let mut inv = Inventory::with_capacity(2);
        assert!(!inv.is_full());
        inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(1))));
        assert!(!inv.is_full());
        inv.set_slot_without_event(1, Some(ItemStack::single(ItemId(2))));
        assert!(inv.is_full());
    }

    // ─── Silent take / set ───────────────────────────────────────────────

    #[test]
    fn take_slot_without_event_empties_the_slot() {
        let mut inv = Inventory::with_capacity(2);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(5))));
        let taken = inv.take_slot_without_event(0).expect("stack present");
        assert_eq!(taken.count.get(), 5);
        assert!(inv.slot(0).is_none());
    }

    #[test]
    fn take_slot_without_event_oob_is_none() {
        let mut inv = Inventory::with_capacity(1);
        assert!(inv.take_slot_without_event(99).is_none());
    }

    #[test]
    fn take_slot_n_splits_when_n_lt_count() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(10))));
        let taken = inv.take_slot_n_without_event(0, 3).expect("split ok");
        assert_eq!(taken.count.get(), 3);
        assert_eq!(inv.slot(0).unwrap().count.get(), 7);
    }

    #[test]
    fn take_slot_n_clears_when_n_ge_count() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(4))));
        let taken = inv.take_slot_n_without_event(0, 99).expect("drain");
        assert_eq!(taken.count.get(), 4);
        assert!(inv.slot(0).is_none());
    }

    #[test]
    fn take_slot_n_zero_is_noop() {
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(5))));
        assert!(inv.take_slot_n_without_event(0, 0).is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 5);
    }

    #[test]
    fn set_slot_returns_previous_occupant() {
        let mut inv = Inventory::with_capacity(1);
        let prev = inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(1))));
        assert!(prev.is_none());
        let prev2 = inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(2))));
        assert_eq!(prev2.unwrap().item, ItemId(1));
    }

    // ─── Silent insert ──────────────────────────────────────────────────

    #[test]
    fn insert_stack_into_empty_uses_first_slot() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        let leftover = inv.insert_stack_without_event(ItemStack::new(ItemId(1), nz(5)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 5);
    }

    #[test]
    fn insert_stack_merges_into_partial() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(60))));
        let leftover = inv.insert_stack_without_event(ItemStack::new(ItemId(1), nz(3)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 63);
    }

    #[test]
    fn insert_stack_overflows_into_next_empty() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(60))));
        // 60 + 10 = 70 > max 64 → 4 stays in slot 0, 6 spills to slot 1
        let leftover = inv.insert_stack_without_event(ItemStack::new(ItemId(1), nz(10)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 64);
        assert_eq!(inv.slot(1).unwrap().count.get(), 6);
    }

    #[test]
    fn insert_stack_returns_leftover_when_full() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(1);
        inv.set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(64))));
        let leftover = inv
            .insert_stack_without_event(ItemStack::new(ItemId(1), nz(5)), &reg)
            .expect("leftover");
        assert_eq!(leftover.count.get(), 5);
        assert_eq!(inv.slot(0).unwrap().count.get(), 64);
    }

    #[test]
    fn insert_stack_non_stackable_one_per_slot() {
        let reg = registry_with_basics();
        let mut inv = Inventory::with_capacity(3);
        // 3 of a non-stackable item (max_stack = 1) should occupy 3 slots.
        let leftover = inv.insert_stack_without_event(ItemStack::new(ItemId(2), nz(3)), &reg);
        assert!(leftover.is_none());
        assert_eq!(inv.slot(0).unwrap().count.get(), 1);
        assert_eq!(inv.slot(1).unwrap().count.get(), 1);
        assert_eq!(inv.slot(2).unwrap().count.get(), 1);
    }

    #[test]
    fn insert_strict_success() {
        let mut inv = Inventory::with_capacity(2);
        inv.insert_stack_strict_without_event(1, ItemStack::single(ItemId(1)))
            .expect("ok");
        assert_eq!(inv.slot(1).unwrap().item, ItemId(1));
    }

    #[test]
    fn insert_strict_slot_occupied() {
        let mut inv = Inventory::with_capacity(2);
        inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(1))));
        let err = inv
            .insert_stack_strict_without_event(0, ItemStack::single(ItemId(2)))
            .unwrap_err();
        assert_eq!(err, InsertError::SlotOccupied { slot: 0 });
    }

    #[test]
    fn insert_strict_out_of_bounds() {
        let mut inv = Inventory::with_capacity(2);
        let err = inv
            .insert_stack_strict_without_event(7, ItemStack::single(ItemId(1)))
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
        // wooden pickaxe (low tier)
        reg.register(
            ItemDefinition::new(ItemId(10), "wood_pick")
                .with_max_stack(nz(1))
                .with_tool(pickaxe, ToolTierId(1)),
        );
        // iron pickaxe (high tier)
        reg.register(
            ItemDefinition::new(ItemId(11), "iron_pick")
                .with_max_stack(nz(1))
                .with_tool(pickaxe, ToolTierId(3)),
        );
        // axe (different kind)
        reg.register(
            ItemDefinition::new(ItemId(12), "axe")
                .with_max_stack(nz(1))
                .with_tool(ToolKindId(2), ToolTierId(2)),
        );
        // dirt block, placeable
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
        inv.set_slot_without_event(2, Some(ItemStack::single(ItemId(11))));
        inv.set_slot_without_event(3, Some(ItemStack::single(ItemId(11))));
        let hit = inv.find_slot(ItemSelector::Exact(ItemId(11)), &reg);
        assert_eq!(hit, Some(2));
    }

    #[test]
    fn find_slot_best_tool_picks_highest_tier() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(4);
        inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(10)))); // wood
        inv.set_slot_without_event(2, Some(ItemStack::single(ItemId(11)))); // iron
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
        inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(12)))); // axe only
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
        inv.set_slot_without_event(1, Some(ItemStack::new(ItemId(20), nz(8))));
        let hit = inv.find_slot(ItemSelector::Placeable(BlockId(7)), &reg);
        assert_eq!(hit, Some(1));
    }

    #[test]
    fn find_slot_placeable_no_match() {
        let reg = registry_with_tools();
        let mut inv = Inventory::with_capacity(3);
        inv.set_slot_without_event(0, Some(ItemStack::single(ItemId(10))));
        let hit = inv.find_slot(ItemSelector::Placeable(BlockId(99)), &reg);
        assert_eq!(hit, None);
    }

    #[test]
    fn find_slot_empty_inventory_is_none_for_all_variants() {
        let reg = registry_with_tools();
        let inv = Inventory::with_capacity(4);
        assert!(
            inv.find_slot(ItemSelector::Exact(ItemId(1)), &reg)
                .is_none()
        );
        assert!(
            inv.find_slot(
                ItemSelector::BestToolFor {
                    kind: ToolKindId(1),
                },
                &reg,
            )
            .is_none()
        );
        assert!(
            inv.find_slot(ItemSelector::Placeable(BlockId(1)), &reg)
                .is_none()
        );
    }

    // ─── Event-firing mutator tests ─────────────────────────────────────

    #[derive(Resource, Default)]
    struct Captured(Vec<InventoryChanged>);

    fn capture_observer(trigger: On<InventoryChanged>, mut captured: ResMut<Captured>) {
        captured.0.push(InventoryChanged {
            entity: trigger.entity,
            changes: trigger.changes.clone(),
        });
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<Captured>();
        app.add_observer(capture_observer);
        app
    }

    #[test]
    fn insert_stack_fires_one_event_with_one_change_per_modified_slot() {
        let mut app = make_app();
        let registry = registry_with_basics();
        let entity = app.world_mut().spawn(Inventory::with_capacity(4)).id();
        // Pre-fill slot 0 partially so insert_stack must merge then overflow.
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(60))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    inv.insert_stack(
                        ItemStack::new(ItemId(1), nz(70)),
                        &registry,
                        &mut commands,
                        entity,
                    );
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1, "exactly one event per call");
        let mut changes = captured.0[0].changes.clone();
        // 60 in slot 0 → 64 (delta 4); spill 64 to slot 1; spill 2 to slot 2.
        assert_eq!(changes.len(), 3);
        // Caller may sort if they need ordering — sort here to assert content.
        changes.sort();
        assert_eq!(changes[0].slot, 0);
        assert_eq!(changes[1].slot, 1);
        assert_eq!(changes[2].slot, 2);
        assert_eq!(changes[0].current.as_ref().unwrap().count.get(), 64);
        assert_eq!(changes[1].current.as_ref().unwrap().count.get(), 64);
        assert_eq!(changes[2].current.as_ref().unwrap().count.get(), 2);
    }

    /// Regression for the previously documented "always sorted by ascending
    /// slot index" claim: the doc no longer guarantees order, so the event
    /// payload may interleave high indices (touched by pass 1) before low
    /// indices (touched by pass 2).  Asserts the *set* of touched slots is
    /// correct without depending on order.
    #[test]
    fn insert_stack_changes_may_be_unordered() {
        let mut app = make_app();
        let registry = registry_with_basics();
        let entity = app.world_mut().spawn(Inventory::with_capacity(6)).id();
        // Pre-fill slot 5 with a partial stack of item 1; slots 0..=4 empty.
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(5, Some(ItemStack::new(ItemId(1), nz(60))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    // 60 + 70: 4 fills slot 5, then 64 fills slot 0, then 2 fills slot 1.
                    inv.insert_stack(
                        ItemStack::new(ItemId(1), nz(70)),
                        &registry,
                        &mut commands,
                        entity,
                    );
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1);
        let changes = &captured.0[0].changes;
        let touched: std::collections::BTreeSet<usize> = changes.iter().map(|c| c.slot).collect();
        assert_eq!(touched, [0_usize, 1, 5].into_iter().collect());
        // Pass 1 touches slot 5 first, so slot 5 appears before slot 0 in
        // the unsorted payload — proving the raw order is *not* ascending.
        assert_eq!(changes[0].slot, 5, "pass-1 records the partial slot first");
    }

    #[test]
    fn insert_stack_emits_at_most_one_change_per_slot() {
        let mut app = make_app();
        let registry = registry_with_basics();
        let entity = app.world_mut().spawn(Inventory::with_capacity(4)).id();
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(60))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    inv.insert_stack(
                        ItemStack::new(ItemId(1), nz(80)),
                        &registry,
                        &mut commands,
                        entity,
                    );
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        let changes = &captured.0[0].changes;
        let unique: std::collections::BTreeSet<usize> = changes.iter().map(|c| c.slot).collect();
        assert_eq!(
            unique.len(),
            changes.len(),
            "every modified slot appears at most once in the event payload"
        );
    }

    #[test]
    fn insert_strict_failure_fires_no_event() {
        let mut app = make_app();
        let entity = app.world_mut().spawn(Inventory::with_capacity(2)).id();
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(0, Some(ItemStack::single(ItemId(1))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let res = inv.insert_stack_strict(
                        0,
                        ItemStack::single(ItemId(2)),
                        &mut commands,
                        entity,
                    );
                    assert!(res.is_err());
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn take_slot_on_empty_fires_no_event() {
        let mut app = make_app();
        let entity = app.world_mut().spawn(Inventory::with_capacity(2)).id();

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let taken = inv.take_slot(0, &mut commands, entity);
                    assert!(taken.is_none());
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn set_slot_replacing_fires_event_with_correct_previous_and_current() {
        let mut app = make_app();
        let entity = app.world_mut().spawn(Inventory::with_capacity(2)).id();
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(0, Some(ItemStack::new(ItemId(1), nz(5))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let prev = inv.set_slot(
                        0,
                        Some(ItemStack::new(ItemId(2), nz(1))),
                        &mut commands,
                        entity,
                    );
                    assert_eq!(prev.unwrap().item, ItemId(1));
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1);
        let change = &captured.0[0].changes[0];
        assert_eq!(change.slot, 0);
        assert_eq!(change.previous.as_ref().unwrap().item, ItemId(1));
        assert_eq!(change.current.as_ref().unwrap().item, ItemId(2));
    }

    #[test]
    fn set_slot_to_identical_fires_no_event() {
        let mut app = make_app();
        let entity = app.world_mut().spawn(Inventory::with_capacity(1)).id();
        let stack = ItemStack::new(ItemId(1), nz(3));
        app.world_mut()
            .get_mut::<Inventory>(entity)
            .unwrap()
            .set_slot_without_event(0, Some(stack.clone()));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut Inventory>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    inv.set_slot(0, Some(stack.clone()), &mut commands, entity);
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }
}
