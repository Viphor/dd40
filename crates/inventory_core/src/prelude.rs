//! Re-exports of every stable public type in `dd40_inventory_core`.
//!
//! ```no_run
//! use dd40_inventory_core::prelude::*;
//! ```

pub use crate::character_ext::CharacterInventoryExt;
pub use crate::inventory::{InsertError, Inventory, InventoryChanged, SlotChange};
pub use crate::plugin::InventoryCorePlugin;
