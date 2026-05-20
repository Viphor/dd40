//! Foundation vocabulary for the dd40 inventory system.
//!
//! # Overview
//!
//! This crate defines three layers around a flat slot-based item
//! container:
//!
//! - [`Inventory`][inventory::Inventory] — pure data.  Holds the slots,
//!   exposes mutators that return per-slot diffs
//!   ([`SlotChange`][inventory::SlotChange]) and do nothing ECS-aware.
//! - [`InventoryComponent`][component::InventoryComponent] — Bevy
//!   [`Component`][bevy::prelude::Component] wrapping an [`Inventory`].
//!   Mutators take `&mut Commands` and the holder entity, and trigger
//!   [`InventoryChanged`][component::InventoryChanged] on every
//!   non-empty diff.  This is the wrapper to use for characters, mobs,
//!   vehicles, dropped item entities — anything keyed by `Entity`.
//! - [`BlockInventory`][block::BlockInventory] — block-cell-attached
//!   wrapper that implements
//!   [`BlockData`][dd40_core::block::BlockData] and fires
//!   [`BlockInventoryChanged`][block::BlockInventoryChanged] keyed on
//!   [`BlockPos`][dd40_core::block::BlockPos].  Use for chests,
//!   hoppers, furnaces, droppers — anything that lives inside a block
//!   cell rather than on an entity.
//!
//! Both wrappers share the same underlying [`Inventory`], so item-flow
//! logic written against `&mut Inventory` works against either.
//!
//! # Why a targeted event, not just `Changed<InventoryComponent>`
//!
//! The per-slot diff carried on [`InventoryChanged`] /
//! [`BlockInventoryChanged`] is the only signal that fires **only** on
//! actual content changes — Bevy's
//! [`Changed<T>`][bevy::ecs::query::Changed] filter triggers on every
//! mutable borrow, including borrows that ended up writing nothing.
//! Observers that need accurate "what moved" telemetry must subscribe
//! to the targeted event.
//!
//! # Escape hatch
//!
//! Both wrappers expose `inventory_mut()` to reach the inner
//! [`Inventory`] directly.  Mutations made through that handle skip
//! event emission entirely.  Useful for pre-spawn population, batch
//! operations, and tests; for player-facing mutations call the
//! event-firing methods on the wrapper.
//!
//! # Usage
//!
//! Add [`InventoryCorePlugin`][plugin::InventoryCorePlugin] once to the
//! [`App`][bevy::prelude::App]; reach for an
//! [`InventoryComponent`][component::InventoryComponent] on a
//! [`CharacterBuilder`][] via the
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

pub mod block;
pub mod character_ext;
pub mod component;
pub mod inventory;
pub mod plugin;
pub mod prelude;
