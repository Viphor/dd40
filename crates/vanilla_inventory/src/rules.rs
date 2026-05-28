//! Pure slot-interaction resolver.
//!
//! Given the current state of the cursor (`held`) and a slot (`slot`),
//! plus the item's max-stack size and the click kind, returns the
//! [`SlotResolution`] describing what the apply layer should do.
//!
//! This module is **pure** — no ECS, no globals, no IO.  Every branch
//! is exercised by unit tests at the bottom of the file.
//!
//! Shift-click resolution requires knowledge of the rest of the
//! inventory (which slot on the other side to merge into); the
//! resolver leaves that to the apply layer by returning
//! [`SlotResolution::ShiftMove`].  Drop-outside is also handled at
//! the apply layer — this resolver only deals with slot-targeted
//! interactions.

use std::num::NonZero;

use dd40_item_core::active_item::ItemStack;

/// Kinds of slot-targeted clicks the resolver handles.
///
/// `DropOutside` is intentionally excluded — it is handled in the
/// apply layer because it does not target a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotClickKind {
    /// Primary click: pick up / drop / swap / merge.
    Left,
    /// Secondary click: pick half / drop one / swap.
    Right,
    /// Shift + primary click: move to the other inventory area.
    Shift,
}

/// Output of [`resolve_slot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotResolution {
    /// The cursor and the slot both reach a new state.
    Mutation {
        /// New cursor contents (`None` clears the cursor).
        new_held: Option<ItemStack>,
        /// New slot contents (`None` empties the slot).
        new_slot: Option<ItemStack>,
    },
    /// The apply layer must move the slot's contents to the first
    /// compatible target on the opposite inventory area.
    ShiftMove {
        /// The slot the move originates from.
        from_slot: u8,
    },
    /// Nothing happens.
    NoOp,
}

/// Resolves a single slot-targeted click into a [`SlotResolution`].
///
/// `max_stack` is the per-item cap from
/// [`ItemDefinition::max_stack`][dd40_item_core::registry::ItemDefinition::max_stack]
/// for whichever item is *relevant* — that is the `held` item when
/// merging into a slot, or the slot item when picking from it.  When
/// both `held` and `slot` are `Some` of different items it does not
/// matter which is passed since the resolver swaps without consulting
/// the cap.  The apply layer is responsible for looking the cap up.
pub fn resolve_slot(
    held: Option<ItemStack>,
    slot: Option<ItemStack>,
    max_stack: NonZero<u16>,
    kind: SlotClickKind,
    from_slot: u8,
) -> SlotResolution {
    match kind {
        SlotClickKind::Left => resolve_left(held, slot, max_stack),
        SlotClickKind::Right => resolve_right(held, slot, max_stack),
        SlotClickKind::Shift => {
            if slot.is_some() {
                SlotResolution::ShiftMove { from_slot }
            } else {
                SlotResolution::NoOp
            }
        }
    }
}

fn resolve_left(
    held: Option<ItemStack>,
    slot: Option<ItemStack>,
    max_stack: NonZero<u16>,
) -> SlotResolution {
    match (held, slot) {
        (None, None) => SlotResolution::NoOp,
        (None, Some(s)) => SlotResolution::Mutation {
            new_held: Some(s),
            new_slot: None,
        },
        (Some(h), None) => SlotResolution::Mutation {
            new_held: None,
            new_slot: Some(h),
        },
        (Some(h), Some(s)) if h.item == s.item => {
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
        (Some(h), Some(s)) => SlotResolution::Mutation {
            new_held: Some(s),
            new_slot: Some(h),
        },
    }
}

fn resolve_right(
    held: Option<ItemStack>,
    slot: Option<ItemStack>,
    max_stack: NonZero<u16>,
) -> SlotResolution {
    match (held, slot) {
        (None, None) => SlotResolution::NoOp,
        (None, Some(s)) => {
            // Take ceil(count / 2) into the cursor; leave floor in slot.
            let take = s.count.get().div_ceil(2);
            let leave = s.count.get() - take;
            let new_held = Some(ItemStack::new(
                s.item,
                NonZero::new(take).expect("take >= 1 since count >= 1"),
            ));
            let new_slot = NonZero::new(leave).map(|c| ItemStack::new(s.item, c));
            SlotResolution::Mutation { new_held, new_slot }
        }
        (Some(h), None) => {
            // Place one item into the empty slot.
            let new_slot = Some(ItemStack::single(h.item));
            let remaining = h.count.get() - 1;
            let new_held = NonZero::new(remaining).map(|c| ItemStack::new(h.item, c));
            SlotResolution::Mutation { new_held, new_slot }
        }
        (Some(h), Some(s)) if h.item == s.item => {
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
        (Some(h), Some(s)) => SlotResolution::Mutation {
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

    // ─── Left click ───────────────────────────────────────────────────

    #[test]
    fn left_empty_into_empty_is_noop() {
        let out = resolve_slot(None, None, nz(64), SlotClickKind::Left, 0);
        assert_eq!(out, SlotResolution::NoOp);
    }

    #[test]
    fn left_pickup_from_slot() {
        let out = resolve_slot(None, Some(stack(1, 7)), nz(64), SlotClickKind::Left, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 7)),
                new_slot: None,
            }
        );
    }

    #[test]
    fn left_drop_into_empty() {
        let out = resolve_slot(Some(stack(1, 5)), None, nz(64), SlotClickKind::Left, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 5)),
            }
        );
    }

    #[test]
    fn left_merge_same_item_fits() {
        let out = resolve_slot(
            Some(stack(1, 10)),
            Some(stack(1, 20)),
            nz(64),
            SlotClickKind::Left,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 30)),
            }
        );
    }

    #[test]
    fn left_merge_same_item_overflows() {
        let out = resolve_slot(
            Some(stack(1, 50)),
            Some(stack(1, 40)),
            nz(64),
            SlotClickKind::Left,
            0,
        );
        // 50 + 40 = 90; slot caps at 64, leftover 26 stays held.
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 26)),
                new_slot: Some(stack(1, 64)),
            }
        );
    }

    #[test]
    fn left_merge_into_full_stack_leaves_held_unchanged() {
        let out = resolve_slot(
            Some(stack(1, 5)),
            Some(stack(1, 64)),
            nz(64),
            SlotClickKind::Left,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 5)),
                new_slot: Some(stack(1, 64)),
            }
        );
    }

    #[test]
    fn left_swap_different_items() {
        let out = resolve_slot(
            Some(stack(1, 3)),
            Some(stack(2, 7)),
            nz(64),
            SlotClickKind::Left,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(2, 7)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }

    // ─── Right click ──────────────────────────────────────────────────

    #[test]
    fn right_empty_into_empty_is_noop() {
        let out = resolve_slot(None, None, nz(64), SlotClickKind::Right, 0);
        assert_eq!(out, SlotResolution::NoOp);
    }

    #[test]
    fn right_take_half_rounds_up() {
        // 7 → take 4, leave 3
        let out = resolve_slot(None, Some(stack(1, 7)), nz(64), SlotClickKind::Right, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }

    #[test]
    fn right_take_half_even_count() {
        // 8 → take 4, leave 4
        let out = resolve_slot(None, Some(stack(1, 8)), nz(64), SlotClickKind::Right, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 4)),
            }
        );
    }

    #[test]
    fn right_take_single_takes_whole() {
        let out = resolve_slot(None, Some(stack(1, 1)), nz(64), SlotClickKind::Right, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 1)),
                new_slot: None,
            }
        );
    }

    #[test]
    fn right_place_one_into_empty_with_remainder() {
        let out = resolve_slot(Some(stack(1, 3)), None, nz(64), SlotClickKind::Right, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 2)),
                new_slot: Some(stack(1, 1)),
            }
        );
    }

    #[test]
    fn right_place_last_into_empty_clears_held() {
        let out = resolve_slot(Some(stack(1, 1)), None, nz(64), SlotClickKind::Right, 0);
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: None,
                new_slot: Some(stack(1, 1)),
            }
        );
    }

    #[test]
    fn right_place_one_onto_matching_slot() {
        let out = resolve_slot(
            Some(stack(1, 5)),
            Some(stack(1, 9)),
            nz(64),
            SlotClickKind::Right,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(1, 4)),
                new_slot: Some(stack(1, 10)),
            }
        );
    }

    #[test]
    fn right_place_onto_full_matching_slot_is_noop() {
        let out = resolve_slot(
            Some(stack(1, 5)),
            Some(stack(1, 64)),
            nz(64),
            SlotClickKind::Right,
            0,
        );
        assert_eq!(out, SlotResolution::NoOp);
    }

    #[test]
    fn right_on_different_item_swaps() {
        let out = resolve_slot(
            Some(stack(1, 3)),
            Some(stack(2, 7)),
            nz(64),
            SlotClickKind::Right,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(2, 7)),
                new_slot: Some(stack(1, 3)),
            }
        );
    }

    // ─── Shift click ──────────────────────────────────────────────────

    #[test]
    fn shift_on_empty_slot_is_noop() {
        let out = resolve_slot(None, None, nz(64), SlotClickKind::Shift, 5);
        assert_eq!(out, SlotResolution::NoOp);
    }

    #[test]
    fn shift_on_filled_slot_returns_shift_move() {
        let out = resolve_slot(
            Some(stack(99, 1)),
            Some(stack(1, 4)),
            nz(64),
            SlotClickKind::Shift,
            5,
        );
        // Held is irrelevant for shift; apply layer takes the slot content.
        assert_eq!(out, SlotResolution::ShiftMove { from_slot: 5 });
    }

    // ─── max_stack < 64 edge case ─────────────────────────────────────

    #[test]
    fn left_merge_respects_low_max_stack() {
        // Eggs cap at 16.
        let out = resolve_slot(
            Some(stack(3, 10)),
            Some(stack(3, 12)),
            nz(16),
            SlotClickKind::Left,
            0,
        );
        assert_eq!(
            out,
            SlotResolution::Mutation {
                new_held: Some(stack(3, 6)),
                new_slot: Some(stack(3, 16)),
            }
        );
    }
}
