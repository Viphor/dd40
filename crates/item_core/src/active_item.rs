//! The per-character [`ActiveItem`] component and its [`ItemStack`] payload.
//!
//! # Role in the architecture
//!
//! [`ActiveItem`] is the **single contract** every gameplay system reads when
//! it asks "what is this character holding right now?".  Mining reads it for
//! the tool kind/tier; placement reads it for the placeable block; future
//! "use item" code paths will read it for consumable / weapon behaviour.
//!
//! Inventory crates (`dd40_vanilla_inventory`, hypothetical `dd40_multi_equip`,
//! etc.) **write** [`ActiveItem`] from whatever internal storage they use —
//! a hotbar index, a slot grid, an AI policy.  Replacing the inventory crate
//! therefore requires no changes to mining, placement, or any other
//! gameplay system.
//!
//! A character without an [`ActiveItem`] component, or with `ActiveItem(None)`,
//! is considered to be holding nothing — bare hands.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::registry::ItemId;

/// A non-empty stack of identical items.
///
/// Inventory slots that are empty store `Option::None` rather than a stack
/// with `count = 0`; a `count = 0` stack is an invariant violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct ItemStack {
    /// Which item this stack holds.
    pub item: ItemId,
    /// How many copies are in the stack.
    ///
    /// Must satisfy `1 <= count <= ItemDefinition::max_stack` for the
    /// referenced item.  Inventory crates are responsible for enforcing this
    /// invariant; consumers may assume it holds.
    pub count: u16,
}

impl ItemStack {
    /// Creates a stack of `count` copies of `item`.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `count == 0` — use `Option::None` to
    /// represent "no stack".
    pub fn new(item: ItemId, count: u16) -> Self {
        debug_assert!(count > 0, "ItemStack must have count >= 1; use Option::None for empty");
        Self { item, count }
    }

    /// Convenience constructor for a single-item stack.
    pub fn single(item: ItemId) -> Self {
        Self::new(item, 1)
    }
}

/// The item a character is currently holding.
///
/// Attach to any [`Character`][dd40_character_core::components::Character]
/// entity.  Gameplay systems read this component to determine behaviour:
///
/// - **Mining** looks up the contained item's
///   [`tool`][crate::registry::ItemDefinition::tool] field for the
///   speed-bonus kind/tier.
/// - **Placement** looks up the contained item's
///   [`placeable`][crate::registry::ItemDefinition::placeable] field for
///   the block to place.
///
/// `ActiveItem(None)` (or no component at all) means bare hands — no tool
/// bonus, nothing to place.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct ActiveItem(pub Option<ItemStack>);

impl ActiveItem {
    /// Convenience constructor for a single-item active stack.
    pub fn single(item: ItemId) -> Self {
        Self(Some(ItemStack::single(item)))
    }

    /// Returns the [`ItemId`] currently held, if any.
    pub fn item(&self) -> Option<ItemId> {
        self.0.map(|s| s.item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_active_item_is_empty() {
        let active = ActiveItem::default();
        assert!(active.0.is_none());
        assert_eq!(active.item(), None);
    }

    #[test]
    fn single_constructs_stack_of_one() {
        let active = ActiveItem::single(ItemId(5));
        let stack = active.0.unwrap();
        assert_eq!(stack.item, ItemId(5));
        assert_eq!(stack.count, 1);
        assert_eq!(active.item(), Some(ItemId(5)));
    }

    #[test]
    #[should_panic(expected = "count >= 1")]
    fn zero_count_stack_panics_in_debug() {
        let _ = ItemStack::new(ItemId(1), 0);
    }
}
