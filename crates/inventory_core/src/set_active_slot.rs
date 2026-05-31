//! [`SetActiveSlot`] — local message requesting a change to a
//! character's `Inventory::active_slot`.
//!
//! The seam: GUI / input crates never write
//! [`Inventory::active_slot`][crate::inventory::Inventory::active_slot]
//! directly.  Instead they describe what the player wants and let the
//! authoritative rules layer apply it — exactly the same shape as
//! [`SlotInteraction`][crate::slot_interaction::SlotInteraction].
//!
//! Producers: input/GUI crates (e.g. `dd40_inventory`'s hotbar
//! key/wheel handlers).
//! Consumers in a networked build:
//!
//! - On the **client**, a network bridge drains the message and ships
//!   it to the server (see
//!   `dd40_network::client::inventory::forward_set_active_slot`).
//! - On the **server**, an inbound bridge re-emits the message after
//!   resolving the controlling character, and the rules plugin's
//!   apply system consumes it to call
//!   [`InventoryComponent::set_active_slot`][crate::component::InventoryComponent::set_active_slot].
//!
//! Out-of-range `slot` values are tolerated — the rules apply path
//! clamps them — but well-behaved producers should respect the
//! recipient inventory's capacity.

use bevy::prelude::{Entity, Message};

/// A player-initiated request to change the active slot.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetActiveSlot {
    /// The character whose inventory the request targets.  On the
    /// client this is the local player; on the server the network
    /// bridge fills it in from `ControlledBy`.
    pub character: Entity,
    /// Desired new active slot index.
    pub slot: u8,
}
