//! System-set ordering for the loose-item pipeline.
//!
//! Downstream crates (spawner, merge, pickup, gfx) anchor their
//! systems to one of these stages so the per-tick ordering is
//! deterministic regardless of which plugins are loaded.

use bevy::prelude::*;

/// Ordered stages of the loose-item pipeline.
///
/// All stages run in [`Update`]. The chain is configured by
/// [`crate::plugin::LooseItemCorePlugin`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LooseItemSet {
    /// Spawn freshly requested loose items (from `DropItems`
    /// messages, world generation, etc.).
    Spawn,
    /// Pull attracted items toward nearby characters.
    Attract,
    /// Resolve pickups and merges based on the contact messages
    /// emitted by `dd40_physics`.
    Resolve,
    /// Tick despawn / cooldown timers and remove expired entities.
    Lifecycle,
}
