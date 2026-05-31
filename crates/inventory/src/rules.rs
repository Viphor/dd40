//! Pure slot-interaction resolvers.
//!
//! Each [`SlotInteractionKind`][dd40_inventory_core::slot_interaction::SlotInteractionKind]
//! variant maps to one resolver function in this module.  Given the
//! current state of the cursor (`held`) and a slot (`slot`), plus the
//! relevant item's max-stack size, each resolver returns a
//! [`SlotResolution`] describing what the apply layer should do.
//!
//! This module is **pure** — no ECS, no globals, no IO.  Every branch
//! is exercised by unit tests at the bottom of the file.
//!
//! Quick-transfer (`QuickTransfer`) and drop-held (`DropHeld`) are
//! handled directly in the apply layer because they require knowledge
//! of the rest of the inventory (quick-transfer) or of the world
//! (drop-held); only the per-slot intents (`TakeAll`, `PlaceAll`,
//! `TakeHalf`, `PlaceOne`) live here.
//!
//! # Intent / cursor-state contract
//!
//! `Take*` resolvers are no-ops when the cursor is non-empty;
//! `Place*` resolvers are no-ops when the cursor is empty.  This is
//! intentional — the GUI is responsible for sending the intent that
//! matches the *current* cursor state, and a mismatch (caused by lag
//! or by a buggy / hostile client) must not silently flip the
//! operation around.  When in doubt the server treats the message as
//! a no-op.

use std::num::NonZero;

use dd40_item_core::active_item::ItemStack;

/// Output of every resolver in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotResolution {
    /// The cursor and the slot both reach a new state.
    Mutation {
        /// New cursor contents (`None` clears the cursor).
        new_held: Option<ItemStack>,
        /// New slot contents (`None` empties the slot).
        new_slot: Option<ItemStack>,
    },
    /// Nothing happens.
    NoOp,
}

/// Resolver for [`TakeAll`][dd40_inventory_core::slot_interaction::SlotInteractionKind::TakeAll].
///
/// Picks up the slot's full stack into the cursor.  No-op when the
/// cursor is non-empty or the slot is empty.
pub fn resolve_take_all(held: Option<ItemStack>, slot: Option<ItemStack>) -> SlotResolution {
    match (held, slot) {
        (None, Some(s)) => SlotResolution::Mutation {
            new_held: Some(s),
            new_slot: None,
        },
        _ => SlotResolution::NoOp,
    }
}

/// Resolver for [`PlaceAll`][dd40_inventory_core::slot_interaction::SlotInteractionKind::PlaceAll].
///
/// Deposits the held stack into `slot`:
///
/// - empty slot → drop the full held stack into the slot.
/// - matching item → merge up to `max_stack`; any leftover stays in
///   the cursor.
/// - different item → swap.
///
/// No-op when the cursor is empty.
pub fn resolve_place_all(
    held: Option<ItemStack>,
    slot: Option<ItemStack>,
    max_stack: NonZero<u16>,
) -> SlotResolution {
    let Some(h) = held else {
        return SlotResolution::NoOp;
    };
    match slot {
        None => SlotResolution::Mutation {
            new_held: None,
            new_slot: Some(h),
        },
        Some(s) if s.item == h.item => {
            let cap = max_stack.get();
            let total = h.count.get() as u32 + s.count.get() as u32;
            let new_slot_count = total.min(cap as u32) as u16;
            let leftover = total - new_slot_count as u32;
            let new_slot = Some(ItemStack::new(
                s.item,
                NonZero::new(new_slot_count).expect("at least s.count, which is non-zero"),
            ));
            let new_held = NonZero::new(leftover as u16).map(|c| ItemStack::new(h.item, c));
            SlotResolution::Mutation { new_held, new_slot }
        }
        Some(s) => SlotResolution::Mutation {
            new_held: Some(s),
            new_slot: Some(h),
        },
    }
}

/// Resolver for [`TakeHalf`][dd40_inventory_core::slot_interaction::SlotInteractionKind::TakeHalf].
///
/// Picks up ceil(slot.count / 2) into the cursor; the remaining
/// floor(count / 2) stays in the slot.  No-op when the cursor is
/// non-empty or the slot is empty.
pub fn resolve_take_half(held: Option<ItemStack>, slot: Option<ItemStack>) -> SlotResolution {
    let (None, Some(s)) = (held, slot) else {
        return SlotResolution::NoOp;
    };
    let take = s.count.get().div_ceil(2);
    let leave = s.count.get() - take;
    let new_held = Some(ItemStack::new(
        s.item,
        NonZero::new(take).expect("take >= 1 since count >= 1"),
    ));
    let new_slot = NonZero::new(leave).map(|c| ItemStack::new(s.item, c));
    SlotResolution::Mutation { new_held, new_slot }
}

/// Resolver for [`PlaceOne`][dd40_inventory_core::slot_interaction::SlotInteractionKind::PlaceOne].
///
/// Deposits a single item from the cursor into `slot`:
///
/// - empty slot → place 1 item; held shrinks by 1.
/// - matching item below `max_stack` → +1 to slot; held shrinks by 1.
/// - matching item already at `max_stack` → no-op.
/// - different item → swap (mirrors the original right-click
///   semantics; the alternative would be to no-op, but swap is what
///   players expect from a Minecraft-style GUI).
///
/// No-op when the cursor is empty.
pub fn resolve_place_one(
    held: Option<ItemStack>,
    slot: Option<ItemStack>,
    max_stack: NonZero<u16>,
) -> SlotResolution {
    let Some(h) = held else {
        return SlotResolution::NoOp;
    };
    match slot {
        None => {
            let new_slot = Some(ItemStack::single(h.item));
            let remaining = h.count.get() - 1;
            let new_held = NonZero::new(remaining).map(|c| ItemStack::new(h.item, c));
            SlotResolution::Mutation { new_held, new_slot }
        }
        Some(s) if s.item == h.item => {
            if s.count.get() >= max_stack.get() {
                return SlotResolution::NoOp;
            }
            let new_slot_count = s.count.get() + 1;
            let new_slot = Some(ItemStack::new(
                s.item,
                NonZero::new(new_slot_count).expect(">= s.count, which is non-zero"),
            ));
            let remaining = h.count.get() - 1;
            let new_held = NonZero::new(remaining).map(|c| ItemStack::new(h.item, c));
            SlotResolution::Mutation { new_held, new_slot }
        }
        Some(s) => SlotResolution::Mutation {
            new_held: Some(s),
            new_slot: Some(h),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_item_core::registry::ItemId;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("nz literal must be > 0")
    }

    fn stack(item: u16, count: u16) -> ItemStack {
        ItemStack::new(ItemId(item), nz(count))
    }

    // ─── TakeAll ──────────────────────────────────────────────────────

    #[test]
    fn take_all_empty_slot_is_noop() {
        assert_eq!(resolve_take_all(None, None), SlotResolution::NoOp);
    }

    #[test]
    fn take_all_picks_up_full_slot() {
        assert_eq!(
            resolve_take_all(None, Some(stack(1, 7))),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 7)),
                new_slot: None,
            }
        );
    }

    #[test]
    fn take_all_with_held_cursor_is_noop() {
        // Wrong intent for the cursor state — must be a no-op so a
        // lagged client can't accidentally swap.
        assert_eq!(
            resolve_take_all(Some(stack(1, 1)), Some(stack(2, 3))),
            SlotResolution::NoOp
        );
    }

    // ─── PlaceAll ─────────────────────────────────────────────────────

    #[test]
    fn place_all_empty_cursor_is_noop() {
        assert_eq!(
            resolve_place_all(None, Some(stack(1, 5)), nz(64)),
            SlotResolution::NoOp
        );
    }

    #[test]
    fn place_all_into_empty_slot() {
        assert_eq!(
            resolve_place_all(Some(stack(1, 5)), None, nz(64)),
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 5)),
            }
        );
    }

    #[test]
    fn place_all_merge_same_item_fits() {
        assert_eq!(
            resolve_place_all(Some(stack(1, 10)), Some(stack(1, 20)), nz(64)),
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 30)),
            }
        );
    }

    #[test]
    fn place_all_merge_same_item_overflows() {
        // 50 + 40 = 90; slot caps at 64, leftover 26 stays held.
        assert_eq!(
            resolve_place_all(Some(stack(1, 50)), Some(stack(1, 40)), nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 26)),
                new_slot: Some(stack(1, 64)),
            }
        );
    }

    #[test]
    fn place_all_merge_into_full_stack_leaves_held_unchanged() {
        assert_eq!(
            resolve_place_all(Some(stack(1, 5)), Some(stack(1, 64)), nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 5)),
                new_slot: Some(stack(1, 64)),
            }
        );
    }

    #[test]
    fn place_all_swap_different_items() {
        assert_eq!(
            resolve_place_all(Some(stack(1, 3)), Some(stack(2, 7)), nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(2, 7)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }

    #[test]
    fn place_all_merge_respects_low_max_stack() {
        // Eggs cap at 16.
        assert_eq!(
            resolve_place_all(Some(stack(3, 10)), Some(stack(3, 12)), nz(16)),
            SlotResolution::Mutation {
                new_held: Some(stack(3, 6)),
                new_slot: Some(stack(3, 16)),
            }
        );
    }

    // ─── TakeHalf ─────────────────────────────────────────────────────

    #[test]
    fn take_half_empty_slot_is_noop() {
        assert_eq!(resolve_take_half(None, None), SlotResolution::NoOp);
    }

    #[test]
    fn take_half_with_held_cursor_is_noop() {
        assert_eq!(
            resolve_take_half(Some(stack(1, 1)), Some(stack(1, 5))),
            SlotResolution::NoOp
        );
    }

    #[test]
    fn take_half_rounds_up() {
        // 7 → take 4, leave 3
        assert_eq!(
            resolve_take_half(None, Some(stack(1, 7))),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }

    #[test]
    fn take_half_even_count() {
        // 8 → take 4, leave 4
        assert_eq!(
            resolve_take_half(None, Some(stack(1, 8))),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 4)),
            }
        );
    }

    #[test]
    fn take_half_single_takes_whole() {
        assert_eq!(
            resolve_take_half(None, Some(stack(1, 1))),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 1)),
                new_slot: None,
            }
        );
    }

    // ─── PlaceOne ─────────────────────────────────────────────────────

    #[test]
    fn place_one_empty_cursor_is_noop() {
        assert_eq!(
            resolve_place_one(None, Some(stack(1, 5)), nz(64)),
            SlotResolution::NoOp
        );
    }

    #[test]
    fn place_one_into_empty_with_remainder() {
        assert_eq!(
            resolve_place_one(Some(stack(1, 3)), None, nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 2)),
                new_slot: Some(stack(1, 1)),
            }
        );
    }

    #[test]
    fn place_one_last_into_empty_clears_held() {
        assert_eq!(
            resolve_place_one(Some(stack(1, 1)), None, nz(64)),
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 1)),
            }
        );
    }

    #[test]
    fn place_one_onto_matching_slot() {
        assert_eq!(
            resolve_place_one(Some(stack(1, 5)), Some(stack(1, 9)), nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 10)),
            }
        );
    }

    #[test]
    fn place_one_onto_full_matching_slot_is_noop() {
        assert_eq!(
            resolve_place_one(Some(stack(1, 5)), Some(stack(1, 64)), nz(64)),
            SlotResolution::NoOp
        );
    }

    #[test]
    fn place_one_on_different_item_swaps() {
        assert_eq!(
            resolve_place_one(Some(stack(1, 3)), Some(stack(2, 7)), nz(64)),
            SlotResolution::Mutation {
                new_held: Some(stack(2, 7)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }
}
