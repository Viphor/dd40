//! Server-side spawning + lifecycle for loose items.
//!
//! # Overview
//!
//! This crate is the **producer half** of the loose-item pipeline:
//!
//! 1. Drains [`DropItems`](dd40_inventory_core::DropItems) messages
//!    and spawns one [`LooseItem`](dd40_loose_item_core::LooseItem)
//!    entity per stack (splitting oversized stacks against
//!    [`ItemDefinition::max_stack`](dd40_item_core::registry::ItemDefinition::max_stack)).
//! 2. Ticks [`DespawnTimer`](dd40_loose_item_core::DespawnTimer) and
//!    [`PickupCooldown`](dd40_loose_item_core::PickupCooldown) every
//!    frame; removes entities whose lifetime has elapsed.
//!
//! Pickup, merging, attraction, visuals, and persistence live in
//! separate crates. Spawned entities carry only physics + the
//! `LooseItem` payload — every other crate slots its behaviour in
//! by querying those same components.
//!
//! # Usage
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_loose_items::plugin::LooseItemsPlugin;
//!
//! App::new().add_plugins(LooseItemsPlugin).run();
//! ```

pub mod plugin;
pub mod spawn;

pub use plugin::LooseItemsPlugin;
