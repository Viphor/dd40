//! Re-exports of every stable public type in `dd40_inventory_core`.
//!
//! ```no_run
//! use dd40_inventory_core::prelude::*;
//! ```

pub use crate::block::{BlockInventory, BlockInventoryChanged};
pub use crate::character_ext::CharacterInventoryExt;
pub use crate::component::{InventoryChanged, InventoryComponent};
pub use crate::drop::DropItems;
pub use crate::held_stack::{HeldStack, HeldStackComponent};
pub use crate::inventory::{InsertError, Inventory, SlotChange};
pub use crate::plugin::InventoryCorePlugin;
pub use crate::selected_slot::{HOTBAR_SIZE, SelectedHotbarSlot};
pub use crate::slot_interaction::{SlotInteraction, SlotInteractionKind};
