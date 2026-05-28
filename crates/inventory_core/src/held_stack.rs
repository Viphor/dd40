//! [`HeldStackComponent`] — the item stack a character is currently
//! dragging with the cursor inside an inventory GUI.
//!
//! Foundation vocabulary.  Mutated by the active inventory rules crate
//! (e.g. `dd40_vanilla_inventory`); read by GUI crates to render the
//! local player's cursor-following stack visual.
//!
//! Per-character component (rather than a global resource) so that the
//! server can be the authority on every player's cursor and so that
//! other clients can observe a remote player's held stack — useful
//! for anti-cheat (server can validate placements against this value)
//! and for future features like rendering a remote player's held item
//! above their head.

use bevy::prelude::{Component, Reflect, ReflectComponent};
use dd40_item_core::active_item::ItemStack;
use serde::{Deserialize, Serialize};

/// Per-character held stack — the cursor stack belonging to the
/// player who controls this `Character`.
///
/// - `Some(stack)` — there is a stack on the cursor; UI crates render
///   it floating at the cursor position and treat clicks outside any
///   slot widget as drop-outside intent.
/// - `None` — the cursor is empty; clicks on slots pick stacks up
///   into the cursor according to the rules of the active inventory
///   crate.
///
/// Cleared automatically by the rules crate on
/// [`crate::drop::DropItems`].
///
/// `SelectedHotbarSlot` deliberately stays a client-local resource —
/// it's pure UI state and does not need to be replicated.
#[derive(Component, Default, Debug, Clone, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct HeldStackComponent(pub Option<ItemStack>);

impl HeldStackComponent {
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
        assert!(HeldStackComponent::default().is_empty());
    }

    #[test]
    fn take_clears() {
        let mut h = HeldStackComponent(Some(ItemStack::new(
            ItemId(1),
            NonZero::new(3).expect("nz"),
        )));
        assert!(h.is_some());
        assert!(h.take().is_some());
        assert!(h.is_empty());
    }
}
