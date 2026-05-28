//! [`SelectedHotbarSlot`] — which hotbar slot the player has highlighted.
//!
//! Foundation vocabulary only.  No system in this crate mutates the
//! component; the active inventory implementation (e.g.
//! `dd40_vanilla_inventory`) owns the read/write logic and the GUI
//! consumes it for highlight rendering and active-item derivation.

use bevy::prelude::*;

/// Number of slots in the player hotbar.
///
/// `dd40_vanilla_inventory` defines the hotbar as `Inventory` slots
/// `0..HOTBAR_SIZE`.  Other inventory crates are free to reinterpret
/// these slots, but GUI crates assume this many cells when rendering
/// the always-visible hotbar.
pub const HOTBAR_SIZE: u8 = 9;

/// The hotbar slot the player currently has selected.
///
/// Range: `0..HOTBAR_SIZE`.  Implementation crates must clamp or wrap
/// when mutating; out-of-range values are a logic error and other
/// crates will treat them as "no selection".
///
/// # Ownership
///
/// - **Written by** the active inventory rules crate
///   (e.g. `dd40_vanilla_inventory`) in response to player input.
/// - **Read by** inventory GUI crates to draw the highlight, and by
///   the inventory rules crate itself to derive
///   [`ActiveItem`][dd40_item_core::active_item::ActiveItem] from the
///   matching `Inventory` slot.
///
/// Attached to the character entity, alongside `InventoryComponent`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct SelectedHotbarSlot(pub u8);

impl Default for SelectedHotbarSlot {
    fn default() -> Self {
        Self(0)
    }
}

impl SelectedHotbarSlot {
    /// Returns the current slot index.
    pub fn get(self) -> u8 {
        self.0
    }

    /// Sets the slot, wrapping into `0..HOTBAR_SIZE`.
    pub fn set_wrapped(&mut self, slot: i16) {
        let size = HOTBAR_SIZE as i16;
        self.0 = slot.rem_euclid(size) as u8;
    }

    /// Shifts the selection by `delta`, wrapping into `0..HOTBAR_SIZE`.
    pub fn shift(&mut self, delta: i16) {
        let next = self.0 as i16 + delta;
        self.set_wrapped(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        assert_eq!(SelectedHotbarSlot::default().get(), 0);
    }

    #[test]
    fn shift_wraps_forward() {
        let mut s = SelectedHotbarSlot(8);
        s.shift(1);
        assert_eq!(s.get(), 0);
        s.shift(2);
        assert_eq!(s.get(), 2);
    }

    #[test]
    fn shift_wraps_backward() {
        let mut s = SelectedHotbarSlot(0);
        s.shift(-1);
        assert_eq!(s.get(), HOTBAR_SIZE - 1);
    }

    #[test]
    fn set_wrapped_handles_large_negatives() {
        let mut s = SelectedHotbarSlot(0);
        s.set_wrapped(-19);
        // -19 mod 9 == -1 → +9 → 8
        assert_eq!(s.get(), 8);
    }
}
