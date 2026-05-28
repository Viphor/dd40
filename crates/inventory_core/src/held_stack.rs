//! [`HeldStack`] — the item stack the player is currently dragging with
//! the cursor inside an inventory GUI.
//!
//! Foundation vocabulary.  Owned by the active inventory rules crate
//! (e.g. `dd40_vanilla_inventory`); read by the GUI crate to render
//! the cursor-following stack visual.
//!
//! `HeldStack` is intentionally a single global `Resource` and not a
//! per-character `Component` because v1 of the inventory GUI is
//! local-only: there is one cursor, therefore one held stack at a
//! time.  Multi-character / split-screen variants will need to
//! revisit this.

use bevy::prelude::Resource;
use dd40_item_core::active_item::ItemStack;

/// The stack the player is currently dragging with the cursor, if any.
///
/// - `Some(stack)` — there is a stack on the cursor; UI crates render
///   it floating at the cursor position and treat clicks outside any
///   slot widget as drop-outside intent.
/// - `None` — the cursor is empty; clicks on slots pick stacks up
///   into the cursor according to the rules of the active inventory
///   crate.
///
/// Cleared automatically by the rules crate on [`crate::drop::DropItems`].
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct HeldStack(pub Option<ItemStack>);

impl HeldStack {
    /// Returns `true` when a stack is currently held.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Returns `true` when no stack is held.
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// Borrows the held stack, if any.
    pub fn get(&self) -> Option<&ItemStack> {
        self.0.as_ref()
    }

    /// Takes the held stack out, leaving the cursor empty.
    pub fn take(&mut self) -> Option<ItemStack> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_item_core::registry::ItemId;
    use std::num::NonZero;

    #[test]
    fn default_is_empty() {
        assert!(HeldStack::default().is_empty());
    }

    #[test]
    fn take_clears() {
        let mut h = HeldStack(Some(ItemStack::new(
            ItemId(1),
            NonZero::new(3).expect("nz"),
        )));
        assert!(h.is_some());
        assert!(h.take().is_some());
        assert!(h.is_empty());
    }
}
