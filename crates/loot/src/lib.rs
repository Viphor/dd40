//! Server-authoritative loot system: converts every accepted
//! [`ChunkChange::Remove`][dd40_core::chunk::ChunkChange::Remove] into
//! [`DropItems`][dd40_inventory_core::drop::DropItems] messages.
//!
//! # Scope
//!
//! This crate is Tier 1 and **server-only**. Adding the plugin is what
//! turns "blocks being mined" into "item drops appearing in the
//! world": no other system spawns drops. Clients do not add this
//! plugin — they receive replicated item entities once the
//! item-entity spawner lands.
//!
//! # Pipeline
//!
//! ```text
//!  PostUpdate
//!  ──────────
//!    ChunkAuthoritySet::Validate
//!      ├─ snapshot_remove_targets   (this crate)
//!      └─ (other validators)
//!    ChunkAuthoritySet::Commit
//!      └─ commit_predicted_changes  (dd40_core)        → ChunkChanged
//!    LootSet::EmitDrops
//!      └─ emit_loot_drops           (this crate)       → DropItems
//! ```
//!
//! `snapshot_remove_targets` runs in `Validate` and records the prior
//! block id and any
//! [`BlockInventory`][dd40_inventory_core::block::BlockInventory]
//! contents of every cell that has a predicted `Remove`. After commit,
//! `emit_loot_drops` matches the snapshot against the actual
//! `ChunkChanged.changes`, so only changes that were **accepted** turn
//! into drops.
//!
//! # Loot resolution order
//!
//! For each removed cell the loot table is resolved by:
//!
//! 1. The cell's own [`LootTable`][dd40_loot_core::table::LootTable]
//!    cell-data, if present.
//! 2. The [`BlockDefinition`][dd40_core::block::BlockDefinition]'s
//!    default [`LootTable`][dd40_loot_core::table::LootTable]
//!    block-data, if present.
//! 3. Fallback: drop a single copy of the
//!    [`ItemId`][dd40_item_core::registry::ItemId] whose
//!    [`placeable`][dd40_item_core::registry::ItemDefinition::placeable]
//!    field points at the removed block. If no item is registered as
//!    "the placement source" for the block, nothing is dropped.
//!
//! Any [`BlockInventory`][dd40_inventory_core::block::BlockInventory]
//! contents are then appended to the rolled drops, and the orphaned
//! [`BlockInventory`][dd40_inventory_core::block::BlockInventory] is
//! cleared from the chunk via a predicted
//! [`CellDataChange`][dd40_core::chunk::CellDataChange] clear that
//! commits next frame.

pub mod plugin;
pub mod prelude;
pub mod system;

pub use plugin::{LootPlugin, LootSet};
