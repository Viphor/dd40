//! Integration crate: loose-item pickup into a character's inventory.
//!
//! This is the **only** crate where [`LooseItem`] and [`Inventory`]
//! meet.  Either one can change independently as long as this thin
//! glue layer keeps up — the pickup behaviour does not bleed into
//! either foundation.
//!
//! # Usage
//!
//! Add [`LooseItemPickupPlugin`] to the **server** binary.  Clients
//! never run the pickup system; they only see the resulting
//! inventory + entity-despawn replication.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_integration_loose_item_pickup::LooseItemPickupPlugin;
//!
//! App::new().add_plugins(LooseItemPickupPlugin).run();
//! ```
//!
//! [`LooseItem`]: dd40_loose_item_core::LooseItem
//! [`Inventory`]: dd40_inventory_core::inventory::Inventory

pub mod attract;
pub mod pickup;
pub mod plugin;

pub use plugin::LooseItemPickupPlugin;
