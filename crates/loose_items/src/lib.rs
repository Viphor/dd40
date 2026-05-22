//! Server-side spawning, lifecycle, and merging for loose items.
//!
//! # Overview
//!
//! This crate is the **producer + maintenance half** of the
//! loose-item pipeline:
//!
//! 1. Drains [`DropItems`](dd40_inventory_core::drop::DropItems)
//!    messages and spawns one
//!    [`LooseItem`](dd40_loose_item_core::LooseItem) entity per
//!    stack (splitting against
//!    [`ItemDefinition::max_stack`](dd40_item_core::registry::ItemDefinition::max_stack)).
//! 2. Subscribes to
//!    [`BodyBodyContact`](dd40_physics_core::messages::BodyBodyContact)
//!    and merges same-item loose items that stay in contact for
//!    [`LooseItemConfig::merge_contact_duration`](dd40_loose_item_core::LooseItemConfig::merge_contact_duration).
//! 3. Ticks
//!    [`DespawnTimer`](dd40_loose_item_core::DespawnTimer) and
//!    [`PickupCooldown`](dd40_loose_item_core::PickupCooldown) every
//!    frame; removes entities whose lifetime has elapsed.
//!
//! Pickup, attraction, visuals and persistence live in separate
//! crates that read these same foundation components.
//!
//! # Usage
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_loose_items::plugin::LooseItemsPlugin;
//!
//! App::new().add_plugins(LooseItemsPlugin).run();
//! ```

pub mod merge;
pub mod plugin;
pub mod spawn;

pub use plugin::LooseItemsPlugin;
