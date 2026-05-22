//! Tunables for the loose-item system.

use bevy::prelude::*;
use std::time::Duration;

/// Server-side configuration for loose items.
///
/// Mutate this resource at startup (or any time) to change the global
/// defaults applied to newly spawned loose items.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct LooseItemConfig {
    /// How long a freshly spawned loose item lives before its
    /// [`crate::DespawnTimer`] expires and the entity is removed.
    ///
    /// Default: **5 minutes**. Keep this finite for performance —
    /// stale items accumulate quickly during combat.
    pub default_lifetime: Duration,

    /// How long a freshly spawned (or dropped) loose item refuses
    /// pickups, populated into [`crate::PickupCooldown`].
    ///
    /// Default: **500 ms**, long enough to prevent the
    /// drop-and-instantly-re-pickup loop without feeling laggy.
    pub default_pickup_cooldown: Duration,

    /// Radius (world units) around a character within which loose
    /// items begin to drift toward them.
    ///
    /// Default: **1.5 m**. Setting this to zero disables attraction.
    pub attraction_radius: f32,

    /// Acceleration (m/s²) applied to an attracted loose item, scaled
    /// linearly by `(1 - distance / attraction_radius)`.
    ///
    /// Default: **12.0 m/s²**.
    pub attraction_strength: f32,

    /// How long two same-item loose items must stay in contact
    /// before they merge into one stack.
    ///
    /// Default: **1 second**.  Anything shorter feels twitchy in a
    /// pile of dropped items; anything longer feels broken.
    pub merge_contact_duration: Duration,
}

impl Default for LooseItemConfig {
    fn default() -> Self {
        Self {
            default_lifetime: Duration::from_secs(5 * 60),
            default_pickup_cooldown: Duration::from_millis(500),
            attraction_radius: 1.5,
            attraction_strength: 12.0,
            merge_contact_duration: Duration::from_secs(1),
        }
    }
}
