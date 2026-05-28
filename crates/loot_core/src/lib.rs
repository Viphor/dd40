//! Foundation vocabulary for the dd40 loot system.
//!
//! # Overview
//!
//! Defines [`LootTable`] — a runtime-rollable list of
//! [`LootEntry`] values that produces a `Vec` of
//! `ItemStack` when rolled against an
//! `RngCore`.  The table itself implements
//! `BlockData` so it can be attached as
//! default block data on a
//! `BlockDefinition` and looked up
//! at break time by a higher-tier loot system (see `dd40_loot`).
//!
//! # Scope
//!
//! `dd40_loot_core` is a Tier 0 foundation crate: it owns the types
//! and the deterministic roll algorithm, but never spawns anything
//! itself.  The actual block-destroyed → `DropItems` pipeline lives in
//! `dd40_loot` (Tier 1, server-only).
//!
//! # Usage
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_loot_core::prelude::*;
//!
//! App::new().add_plugins(LootCorePlugin).run();
//! ```

pub mod plugin;
pub mod prelude;
pub mod table;

pub use plugin::LootCorePlugin;
pub use table::{LootEntry, LootMode, LootTable};
