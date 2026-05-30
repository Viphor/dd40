//! Hotbar layout constants shared between the vanilla inventory rules
//! crate and the inventory GUI.
//!
//! The hotbar is a convention layered on top of [`Inventory`]: slots
//! `0..HOTBAR_SIZE` of every player's `InventoryComponent` are
//! rendered as the always-visible bottom row.  No type in this crate
//! enforces or reads the hotbar — it exists purely so the GUI and the
//! vanilla input handlers agree on how many cells to render and on
//! which slot indices the number keys map to.
//!
//! The selected hotbar slot is **not** a separate component.  It is
//! stored on [`Inventory::active_slot`][crate::inventory::Inventory::active_slot]
//! and travels with [`InventoryComponent`][crate::component::InventoryComponent]
//! replication.
//!
//! [`Inventory`]: crate::inventory::Inventory

/// Number of slots in the player hotbar.
///
/// `dd40_vanilla_inventory` defines the hotbar as `Inventory` slots
/// `0..HOTBAR_SIZE`.  Other inventory crates are free to reinterpret
/// these slots, but GUI crates assume this many cells when rendering
/// the always-visible hotbar.
pub const HOTBAR_SIZE: u8 = 9;
