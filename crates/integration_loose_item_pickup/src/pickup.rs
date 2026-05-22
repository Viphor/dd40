//! Pickup system: turn `BodyBodyContact` between a character with
//! [`Inventory`] and a [`LooseItem`] into an inventory insert.
//!
//! # Behaviour
//!
//! 1. Each tick, walk the [`BodyBodyContact`] message stream and
//!    collect candidate `(character, loose_item)` pairs where:
//!    - one side is a [`Character`] with an [`Inventory`];
//!    - the other side is a [`LooseItem`];
//!    - the [`LooseItem`]'s [`PickupCooldown`] has elapsed.
//!
//! 2. Multi-character tie-break: when several eligible characters
//!    contact the same [`LooseItem`] in the same tick, the one
//!    whose [`PhysicsPosition`] is closest (squared distance) to
//!    the item's [`PhysicsPosition`] wins.  Exact ties go to the
//!    lower [`Entity::index()`].
//!
//! 3. Try [`Inventory::insert_stack`].  If the entire stack fits,
//!    despawn the loose entity.  If only part fits, shrink the
//!    loose stack to the leftover and leave it on the ground.  If
//!    nothing fits, the entity is untouched.
//!
//! [`Character`]: dd40_character_core::Character
//! [`Inventory`]: dd40_inventory_core::inventory::Inventory
//! [`LooseItem`]: dd40_loose_item_core::LooseItem
//! [`PickupCooldown`]: dd40_loose_item_core::PickupCooldown
//! [`PhysicsPosition`]: dd40_physics_core::PhysicsPosition

use std::collections::HashMap;

use bevy::prelude::*;

use dd40_character_core::components::Character;
use dd40_inventory_core::component::InventoryComponent;
use dd40_item_core::registry::ItemRegistry;
use dd40_loose_item_core::{LooseItem, PickupCooldown};
use dd40_physics_core::messages::BodyBodyContact;
use dd40_physics_core::prelude::PhysicsPosition;

/// Drains [`BodyBodyContact`] and resolves character ↔ loose-item
/// pickups for the current tick.
///
/// Runs in
/// [`LooseItemSet::Resolve`](dd40_loose_item_core::system_sets::LooseItemSet::Resolve).
pub fn pickup_loose_items(
    mut contacts: MessageReader<BodyBodyContact>,
    registry: Res<ItemRegistry>,
    character_q: Query<&PhysicsPosition, (With<Character>, With<InventoryComponent>)>,
    mut inventories: Query<&mut InventoryComponent>,
    mut loose_set: ParamSet<(
        Query<(&LooseItem, &PhysicsPosition, &PickupCooldown)>,
        Query<&mut LooseItem>,
    )>,
    mut commands: Commands,
) {
    let mut best: HashMap<Entity, (Entity, f32)> = HashMap::new();

    {
        let loose_q = loose_set.p0();

        for contact in contacts.read() {
            let (character, loose) = match (
                classify(contact.a, &character_q, &loose_q),
                classify(contact.b, &character_q, &loose_q),
            ) {
                (Endpoint::Character(c), Endpoint::Loose(l, _, cooldown))
                    if cooldown.0.is_finished() =>
                {
                    (c, l)
                }
                (Endpoint::Loose(l, _, cooldown), Endpoint::Character(c))
                    if cooldown.0.is_finished() =>
                {
                    (c, l)
                }
                _ => continue,
            };

            let Ok(char_pos) = character_q.get(character) else {
                continue;
            };
            let Ok((_, loose_pos, _)) = loose_q.get(loose) else {
                continue;
            };
            let dist_sq = char_pos.0.distance_squared(loose_pos.0);

            best.entry(loose)
                .and_modify(|(current, current_dist)| {
                    if dist_sq < *current_dist
                        || (dist_sq == *current_dist && character.index() < current.index())
                    {
                        *current = character;
                        *current_dist = dist_sq;
                    }
                })
                .or_insert((character, dist_sq));
        }
    }

    let mut loose_mut = loose_set.p1();
    for (loose_entity, (character_entity, _)) in best {
        let Ok(loose_view) = loose_mut.get(loose_entity) else {
            continue;
        };
        let stack = loose_view.stack;
        let Ok(mut inventory) = inventories.get_mut(character_entity) else {
            continue;
        };

        let leftover = inventory.insert_stack(stack, &registry, &mut commands, character_entity);
        match leftover {
            None => {
                commands.entity(loose_entity).despawn();
            }
            Some(remaining) if remaining.count == stack.count => {}
            Some(remaining) => {
                if let Ok(mut loose) = loose_mut.get_mut(loose_entity) {
                    loose.stack = remaining;
                }
            }
        }
    }
}

#[allow(dead_code)]
enum Endpoint<'a> {
    Character(Entity),
    Loose(Entity, &'a PhysicsPosition, &'a PickupCooldown),
    Neither,
}

fn classify<'a>(
    entity: Entity,
    character_q: &Query<&PhysicsPosition, (With<Character>, With<InventoryComponent>)>,
    loose_q: &'a Query<(&LooseItem, &PhysicsPosition, &PickupCooldown)>,
) -> Endpoint<'a> {
    if character_q.contains(entity) {
        Endpoint::Character(entity)
    } else if let Ok((_, pos, cooldown)) = loose_q.get(entity) {
        Endpoint::Loose(entity, pos, cooldown)
    } else {
        Endpoint::Neither
    }
}
