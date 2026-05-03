//! Foundation vocabulary for the dd40 item system.
//!
//! # Overview
//!
//! This crate defines the *types* every other crate uses when talking about
//! items: identifiers, definitions, the per-character "currently active item"
//! component, and the messages inventory crates exchange.  It contains no
//! game behaviour and no inventory layout — those live in implementation
//! crates such as `dd40_vanilla_inventory`.
//!
//! # The moddable seam
//!
//! Gameplay systems (mining, placement, "use item") read a single component:
//! [`ActiveItem`][plugin::ActiveItem]. Inventory crates write that component
//! from whatever internal storage they prefer, and may handle
//! [`RequestActiveItem`][plugin::RequestActiveItem] to switch what is active.
//! Replacing the inventory crate therefore requires no changes elsewhere.
//!
//! [`ActiveItem`]: crate::active_item::ActiveItem
//! [`RequestActiveItem`]: crate::plugin::RequestActiveItem

pub mod active_item;
pub mod messages;
pub mod plugin;
pub mod prelude;
pub mod registry;
