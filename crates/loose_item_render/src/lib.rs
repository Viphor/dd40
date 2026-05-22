//! Client-side visuals for [`LooseItem`] entities.
//!
//! [`LooseItemRenderPlugin`] attaches a small spinning, bobbing cube
//! to every [`LooseItem`] the client sees.  The cube's colour is
//! resolved with the following fallback chain:
//!
//! 1. The item's own custom render (TODO — when [`ItemDefinition`]
//!    grows `mesh` / `texture` fields, use them).
//! 2. The colour of the [placeable](dd40_item_core::registry::ItemDefinition::placeable)
//!    block the item maps to.
//! 3. A neutral grey fallback.
//!
//! The visual lives on a **child entity** so the spin + bob animation
//! never fights the network bridge that writes the parent
//! `Transform.translation` from the interpolated
//! [`LooseItemPosition`](https://docs.rs/dd40_network) every frame.
//!
//! [`LooseItem`]: dd40_loose_item_core::LooseItem
//! [`ItemDefinition`]: dd40_item_core::registry::ItemDefinition

pub mod plugin;

pub use plugin::LooseItemRenderPlugin;
