//! Player inventory GUI: the always-visible hotbar, the toggleable
//! 3×9 grid window, item icons, and click-to-`SlotInteraction` input.
//!
//! # Overview
//!
//! This crate owns the bevy_ui presentation layer for the player's
//! `InventoryComponent`. It does **not** mutate inventories itself —
//! every user gesture is published as a
//! [`SlotInteraction`](dd40_inventory_core::slot_interaction::SlotInteraction)
//! message and resolved by whichever inventory-rules crate the binary
//! wires in (the vanilla rules live in `dd40_inventory`).
//!
//! # Usage
//!
//! Add [`plugin::InventoryGuiPlugin`] to your [`bevy::prelude::App`]:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_inventory_gui::plugin::InventoryGuiPlugin;
//!
//! App::new()
//!     .add_plugins(InventoryGuiPlugin)
//!     .run();
//! ```
//!
//! The crate is client-only — it should only be installed on the
//! client binary, never on the dedicated server.  Gestures travel
//! over the wire as `NetSlotInteraction` (see
//! `dd40_network::ClientInventoryNetworkPlugin`); the server runs
//! `dd40_inventory::InventoryRulesPlugin` to apply
//! them and the result replicates back as `InventoryComponent` +
//! `HeldStackComponent`.

pub mod block_icon;
pub mod grid;
pub mod held;
pub mod hotbar;
pub mod icons;
pub mod input;
pub mod plugin;
pub mod slot_widget;
#[cfg(feature = "textures")]
pub mod textures;
pub mod tooltip;

pub use plugin::{InventoryGuiOpen, InventoryGuiPlugin, InventoryGuiSet};
