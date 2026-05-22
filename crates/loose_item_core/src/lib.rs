//! Foundation vocabulary for **loose items** — item stacks that
//! exist in the world as standalone entities (dropped from killed
//! mobs, thrown by players, scattered from broken chests).
//!
//! # Overview
//!
//! This crate is the shared language for everything that interacts
//! with loose items: the spawner, pickup integration, attraction,
//! visuals, persistence, and replication. It defines the [`LooseItem`]
//! component, its supporting timers, the [`LooseItemConfig`] resource,
//! and the [`LooseItemSet`] system-set ordering that downstream crates
//! anchor their systems against.
//!
//! No game systems live here. The actual spawning, merging, and
//! pickup behaviour is supplied by separate implementation crates
//! (`dd40_loose_items`, `dd40_integration_loose_item_pickup`, …).
//!
//! # Usage
//!
//! Add [`plugin::LooseItemCorePlugin`] to your [`App`] to register the
//! types and resource defaults:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_loose_item_core::plugin::LooseItemCorePlugin;
//!
//! App::new().add_plugins(LooseItemCorePlugin).run();
//! ```

pub mod components;
pub mod plugin;
pub mod prelude;
pub mod resources;
pub mod system_sets;

pub use components::{DespawnTimer, LooseItem, PickupCooldown};
pub use plugin::LooseItemCorePlugin;
pub use resources::LooseItemConfig;
pub use system_sets::LooseItemSet;
