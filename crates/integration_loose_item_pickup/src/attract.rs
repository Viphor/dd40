//! Attraction stage — drift loose items toward nearby characters.
//!
//! For every loose item whose [`PickupCooldown`] has elapsed, this system
//! looks for the closest character (with an [`InventoryComponent`] that has
//! at least one free slot) within
//! [`LooseItemConfig::attraction_radius`](LooseItemConfig).  If one is found,
//! an [`Impulse`] is added so the item drifts toward that character.
//!
//! The impulse magnitude is
//! `attraction_strength * (1 - distance / attraction_radius) * dt`, giving a
//! smooth falloff toward the edge of the radius. The system is a no-op when
//! `attraction_radius` is `0.0`.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_inventory_core::component::InventoryComponent;
use dd40_loose_item_core::{LooseItem, LooseItemConfig, PickupCooldown};
use dd40_physics_core::components::{Impulse, PhysicsPosition};

/// Drifts loose items toward the nearest eligible character.
///
/// Runs in
/// [`LooseItemSet::Attract`](dd40_loose_item_core::system_sets::LooseItemSet::Attract).
#[allow(clippy::type_complexity)]
pub fn attract_loose_items(
    time: Res<Time>,
    config: Res<LooseItemConfig>,
    character_q: Query<(&PhysicsPosition, &InventoryComponent), With<Character>>,
    mut loose_q: Query<
        (&PhysicsPosition, &PickupCooldown, &mut Impulse),
        (With<LooseItem>, Without<Character>),
    >,
) {
    if config.attraction_radius <= 0.0 || config.attraction_strength == 0.0 {
        return;
    }

    let dt = time.delta_secs();
    if dt == 0.0 {
        return;
    }

    let radius = config.attraction_radius;
    let radius_sq = radius * radius;

    for (loose_pos, cooldown, mut impulse) in &mut loose_q {
        if !cooldown.0.is_finished() {
            continue;
        }

        let mut best: Option<(Vec3, f32)> = None;
        for (char_pos, inventory) in &character_q {
            if inventory.inventory().is_full() {
                continue;
            }
            let delta = char_pos.0 - loose_pos.0;
            let dist_sq = delta.length_squared();
            if dist_sq > radius_sq {
                continue;
            }
            match &best {
                Some((_, current_dist_sq)) if dist_sq >= *current_dist_sq => {}
                _ => best = Some((delta, dist_sq)),
            }
        }

        let Some((delta, dist_sq)) = best else {
            continue;
        };
        if dist_sq == 0.0 {
            continue;
        }
        let dist = dist_sq.sqrt();
        let direction = delta / dist;
        let falloff = (1.0 - dist / radius).max(0.0);
        impulse.0 += direction * config.attraction_strength * falloff * dt;
    }
}
