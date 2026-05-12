//! Foundation vocabulary for the dd40 inventory system.
//!
//! # Overview
//!
//! This crate defines the [`Inventory`][inventory::Inventory] component — a
//! flat, fixed-capacity container of [`ItemStack`][dd40_item_core::ItemStack]
//! slots — and the [`InventoryChanged`][inventory::InventoryChanged] event
//! emitted whenever its contents are mutated through the public API.  It is
//! a passive container only; selection, hotbar, drag-and-drop, and any
//! `RequestActiveItem` handling live in higher-tier consumer crates.
//!
//! # Why an event, not just `Changed<Inventory>`
//!
//! Mutating methods on [`Inventory`][inventory::Inventory] take
//! `&mut Commands` and the holder [`Entity`][bevy::prelude::Entity], and
//! trigger a targeted [`InventoryChanged`][inventory::InventoryChanged]
//! event after every successful change.  Observers therefore run only when
//! something actually changes, and receive a per-slot diff
//! ([`SlotChange`][inventory::SlotChange]) describing exactly what moved.
//!
//! Bevy's [`Changed<Inventory>`][bevy::ecs::query::Changed] filter still
//! works (it is a free side-effect of the component change tick) and is a
//! reasonable fallback for "refresh everything" consumers, but observers
//! avoid the per-frame iteration cost.
//!
//! # Escape hatch
//!
//! Every event-firing mutator has a `*_without_event` counterpart for use
//! in tests, pre-spawn population, and batch operations where a single
//! summary event is preferable.  Mirrors the
//! [`BlockRegistry::register`][dd40_core::block::registry::BlockRegistry::register] /
//! [`register_without_event`][dd40_core::block::registry::BlockRegistry::register_without_event]
//! precedent in `dd40_core`.
//!
//! # Usage
//!
//! Add [`InventoryCorePlugin`][plugin::InventoryCorePlugin] once to the
//! [`App`][bevy::prelude::App]; reach for an [`Inventory`][inventory::Inventory]
//! on a [`CharacterBuilder`][] via the
//! [`CharacterInventoryExt`][character_ext::CharacterInventoryExt::with_inventory]
//! extension trait.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_inventory_core::prelude::*;
//!
//! App::new()
//!     .add_plugins(InventoryCorePlugin)
//!     .run();
//! ```
//!
//! [`CharacterBuilder`]: https://docs.rs/dd40_character_core

pub mod character_ext;
pub mod inventory;
pub mod plugin;
pub mod prelude;
