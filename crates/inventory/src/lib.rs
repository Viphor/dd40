//! Vanilla inventory rules: slot-mutation resolver, hotbar selection,
//! and the [`RequestActiveItem`][dd40_item_core::messages::RequestActiveItem]
//! bridge.
//!
//! # Overview
//!
//! This crate is the **rules** half of the dd40 player inventory.  It
//! consumes player intent (`SlotInteraction` from a GUI crate, hotbar
//! BEI actions from `dd40_player_input`) and mutates
//! `InventoryComponent` + the per-character `HeldStackComponent`
//! accordingly.  It owns no UI;
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
//! This crate is split into two plugins so the server can run the
//! mutating rules without dragging in client-only selection state:
//!
//! - [`plugin::InventoryPlugin`] — hotbar selection,
//!   `ActiveItem` bridge.  Add on both client and server.
//! - [`plugin::InventoryRulesPlugin`] — the apply system that
//!   mutates `InventoryComponent` / `HeldStackComponent`.  Add on the
//!   **server only** in a networked build.  In a single-player binary,
//!   add it on the client too.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_inventory::plugin::{
//!     InventoryPlugin, InventoryRulesPlugin,
//! };
//!
//! App::new()
//!     .add_plugins((InventoryPlugin, InventoryRulesPlugin))
//!     .run();
//! ```
//!
//! # Multiplayer contract
//!
//! This crate is multiplayer-safe by construction:
//!
//! - Every system that reads or writes hotbar / active-item state
//!   filters with `With<Player>`.  The `Player` marker is added only
//!   to the locally-predicted character (see
//!   `dd40_network::with_predicted_local_player`); replicated remote
//!   characters carry only `Character`, never `Player`.
//! - Auto-attached components (`SelectedHotbarSlot`, `ActiveItem`) are
//!   only inserted on entities with `Added<Player>`, so remote
//!   characters never accrue local-only state.
//! - Entity-addressed messages (`SlotInteraction`,
//!   `RequestActiveItem`) target by `Entity`, so the caller is
//!   responsible for naming the local player explicitly.
//!
//! Tests in `tests/selection_and_apply.rs` lock these invariants in.

pub mod active_item;
pub mod apply;
pub mod plugin;
pub mod rules;
pub mod selection;

pub use plugin::{
    InventoryActiveItemPlugin, InventoryPlugin, InventoryRulesPlugin,
};
