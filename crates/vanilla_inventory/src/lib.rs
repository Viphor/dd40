//! Vanilla inventory rules: slot-mutation resolver, hotbar selection,
//! and the [`RequestActiveItem`][dd40_item_core::messages::RequestActiveItem]
//! bridge.
//!
//! # Overview
//!
//! This crate is the **rules** half of the dd40 player inventory.  It
//! consumes player intent (`SlotInteraction` from a GUI crate, hotbar
//! BEI actions from `dd40_player_input`) and mutates
//! `InventoryComponent` + `HeldStack` accordingly.  It owns no UI;
//! the inventory GUI is a separate Tier 1 crate
//! (`dd40_inventory_gui`) that talks to this crate only through the
//! foundation vocabulary in `dd40_inventory_core`.
//!
//! # Responsibilities
//!
//! - Pure slot-mutation resolver ([`rules`]) — testable without any
//!   ECS state.
//! - Apply system that reads
//!   [`SlotInteraction`][dd40_inventory_core::slot_interaction::SlotInteraction]
//!   messages, runs the resolver, mutates the target inventory, and
//!   emits [`DropItems`][dd40_inventory_core::drop::DropItems] on
//!   drop-outside.
//! - Hotbar selection from the [`HotbarSelect`][dd40_input_core::actions::HotbarSelect]
//!   BEI action (keys 1–9, scroll wheel).
//! - `RequestActiveItem` bridge: emits a request whenever the selected
//!   hotbar slot or its contents change.
//!
//! # Usage
//!
//! Add [`plugin::VanillaInventoryPlugin`] to the client app once.  In
//! v1, inventory is local-only, so the server does not load this
//! plugin.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_vanilla_inventory::plugin::VanillaInventoryPlugin;
//!
//! App::new()
//!     .add_plugins(VanillaInventoryPlugin)
//!     .run();
//! ```

pub mod apply;
pub mod plugin;
pub mod rules;
pub mod selection;

pub use plugin::VanillaInventoryPlugin;
