//! Components attached to loose-item entities.
//!
//! All three components are server-authoritative when running networked
//! — clients receive [`LooseItem`] through replication but the timers
//! ([`DespawnTimer`], [`PickupCooldown`]) tick only on the server.

use bevy::prelude::*;
use bevy::time::Timer;
use dd40_item_core::active_item::ItemStack;

/// Marker + payload for an item stack lying in the world.
///
/// Attach this to any entity you want to behave like a "ground item":
/// physics will move it, the pickup integration will grant it to a
/// touching character, and the merge system will combine touching
/// stacks of the same item type after a short contact period.
///
/// The [`stack`](LooseItem::stack) field is the single source of truth
/// for the item identity and count; visuals and pickup hooks read it
/// directly.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct LooseItem {
    /// The item stack this entity represents.
    pub stack: ItemStack,
}

impl LooseItem {
    /// Convenience constructor.
    pub fn new(stack: ItemStack) -> Self {
        Self { stack }
    }
}

/// How long the loose item has left before it disappears from the
/// world.
///
/// Defaults to [`crate::LooseItemConfig::default_lifetime`] at spawn
/// time. When two stacks merge, the resulting entity inherits the
/// **larger** input's timer.
///
/// The timer ticks only on the server; clients do not observe this
/// component.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct DespawnTimer(pub Timer);

/// Brief gate after spawn (or after being dropped by a character)
/// during which the loose item refuses pickups.
///
/// Without this gate, an item dropped by a player would be picked
/// straight back up on the same tick. The timer is short — a few
/// hundred milliseconds — and counts down on the server only.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct PickupCooldown(pub Timer);
