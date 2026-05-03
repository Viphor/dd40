//! The cross-crate messages that let inventories and selectors talk without
//! depending on each other.
//!
//! # Two flavours, two purposes
//!
//! - [`RequestActiveItem`] is a **Message**: queued, drained by a system in
//!   the inventory crate.  Many requests can pile up in a frame and the
//!   inventory decides which (if any) to honour.
//! - [`ActiveItemChanged`] is an **Event**: observed immediately when an
//!   inventory crate swaps the [`ActiveItem`][crate::active_item::ActiveItem]
//!   on a specific character.  HUDs, networking, and audio can react in the
//!   same tick.
//!
//! # The selector enum
//!
//! [`ItemSelector`] is intentionally a small closed enum, **not** a closure,
//! so requests can be serialised over the network and validated server-side.
//! Add new variants here as new request kinds emerge.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::registry::ItemId;
use dd40_core::block::BlockId;
use dd40_core::tools::ToolKindId;

/// Which item to make active.
///
/// Inventory crates handling [`RequestActiveItem`] interpret the variants:
///
/// - [`Exact`][Self::Exact] — switch to the slot holding exactly this item,
///   if any.
/// - [`BestToolFor`][Self::BestToolFor] — switch to the slot holding the
///   tool with the highest tier multiplier matching the given kind.  Used by
///   `dd40_auto_tool_swap`.
/// - [`Placeable`][Self::Placeable] — switch to the slot holding an item
///   whose `placeable` is this block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub enum ItemSelector {
    /// Match an exact [`ItemId`].
    Exact(ItemId),
    /// Match the best tool of the given kind currently in the inventory.
    BestToolFor {
        /// The tool kind a block prefers (from `BlockDefinition::preferred_tool`).
        kind: ToolKindId,
    },
    /// Match an item that places the given block.
    Placeable(BlockId),
}

/// Request that the active item on `entity` be switched to one matching
/// `selector`.
///
/// Inventory crates drain this message stream and apply changes; if no
/// matching item is found, the request is dropped silently.  Multiple
/// requests in the same frame are processed in order; the last successful
/// match wins.
#[derive(Message, Debug, Clone, Copy)]
pub struct RequestActiveItem {
    /// The character whose active item should change.
    pub entity: Entity,
    /// Which item to switch to.
    pub selector: ItemSelector,
}

/// Emitted by an inventory crate when it changes a character's
/// [`ActiveItem`][crate::active_item::ActiveItem].
///
/// Fires once per actual change.  Listeners include HUDs (refresh the
/// hotbar slot indicator) and networking (replicate the change to remote
/// observers).
#[derive(Event, Debug, Clone, Copy)]
pub struct ActiveItemChanged {
    /// The character whose active item changed.
    pub entity: Entity,
    /// The previous item, or `None` if the slot was empty.
    pub previous: Option<ItemId>,
    /// The new item, or `None` if the slot is now empty.
    pub current: Option<ItemId>,
}
