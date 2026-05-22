//! Same-item loose-item merge.
//!
//! When two [`LooseItem`] entities of the same item type stay in
//! [`BodyBodyContact`] for at least
//! [`LooseItemConfig::merge_contact_duration`] they collapse into a
//! single entity (with spillover if the combined count exceeds
//! `max_stack`).
//!
//! ## Behaviour
//!
//! - Pairs are accumulated in [`MergeAccumulator`].  Entries only
//!   grow on ticks they are observed; pairs not seen this tick are
//!   dropped (a single-tick gap resets the merge clock).
//! - At the threshold, the **larger** stack absorbs the smaller.
//!   On a tie, the lower [`Entity::index()`] (the `a` of the
//!   canonicalised pair) wins.
//! - The keeper's [`DespawnTimer`] is preserved as-is.  Spillover
//!   stays on the absorbed entity (which is not despawned in that
//!   case) so its existing timer continues to tick.
//! - Pickup cooldowns are unaffected — the keeper's own cooldown
//!   is whatever was already on it.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;

use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;
use dd40_loose_item_core::{DespawnTimer, LooseItem, LooseItemConfig};
use dd40_physics_core::messages::BodyBodyContact;

/// Per-pair contact-duration tracker. One global resource keyed by
/// canonicalised `(a, b)` entity pairs where `a.index() < b.index()`.
///
/// Entries grow each tick the pair is observed in
/// [`BodyBodyContact`] and are removed on the first tick they are
/// not. The merge fires the moment an entry reaches
/// [`LooseItemConfig::merge_contact_duration`].
#[derive(Resource, Default, Debug)]
pub struct MergeAccumulator {
    pairs: HashMap<(Entity, Entity), Duration>,
}

impl MergeAccumulator {
    /// Read-only view, exposed for tests and debug overlays.
    pub fn elapsed(&self, a: Entity, b: Entity) -> Option<Duration> {
        let key = canonical_pair(a, b);
        self.pairs.get(&key).copied()
    }
}

fn canonical_pair(a: Entity, b: Entity) -> (Entity, Entity) {
    if a.index() <= b.index() {
        (a, b)
    } else {
        (b, a)
    }
}

/// Drains [`BodyBodyContact`] for same-item [`LooseItem`] pairs,
/// accumulates contact time, and merges pairs that reach the
/// configured threshold.
///
/// Runs in
/// [`LooseItemSet::Resolve`](dd40_loose_item_core::LooseItemSet::Resolve)
/// — after physics has produced the tick's contacts.
pub fn accumulate_and_merge_loose_items(
    mut contacts: MessageReader<BodyBodyContact>,
    time: Res<Time>,
    config: Res<LooseItemConfig>,
    registry: Res<ItemRegistry>,
    mut accumulator: ResMut<MergeAccumulator>,
    mut loose_q: Query<(&mut LooseItem, &mut DespawnTimer)>,
    mut commands: Commands,
) {
    let dt = time.delta();
    let threshold = config.merge_contact_duration;

    let mut seen_this_tick: HashMap<(Entity, Entity), ()> = HashMap::new();

    for contact in contacts.read() {
        let Ok([(item_a, _), (item_b, _)]) = loose_q.get_many([contact.a, contact.b]) else {
            continue;
        };
        if item_a.stack.item != item_b.stack.item {
            continue;
        }
        seen_this_tick.insert((contact.a, contact.b), ());
    }

    // Drop entries we did not see this tick (single-tick gap resets the clock).
    accumulator
        .pairs
        .retain(|key, _| seen_this_tick.contains_key(key));

    // Decide which pairs are ready to merge before we mutate any
    // entities; that way we never get caught mid-iteration with stale
    // accumulator state.
    let mut to_merge: Vec<(Entity, Entity)> = Vec::new();
    for key in seen_this_tick.keys() {
        let entry = accumulator.pairs.entry(*key).or_insert(Duration::ZERO);
        *entry = entry.saturating_add(dt);
        if *entry >= threshold {
            to_merge.push(*key);
        }
    }

    for (a, b) in to_merge {
        accumulator.pairs.remove(&(a, b));

        let Ok([(item_a, timer_a), (item_b, timer_b)]) = loose_q.get_many_mut([a, b]) else {
            continue;
        };
        if item_a.stack.item != item_b.stack.item {
            continue;
        }

        // Pick the keeper: larger count wins; tie-break by lowest
        // index.  `a` already has the lower index by canonicalisation.
        let a_is_keeper = item_a.stack.count >= item_b.stack.count;
        let (mut keeper_item, donor_entity, mut donor_item) = if a_is_keeper {
            let _ = timer_b;
            let _ = timer_a; // keeper's timer is preserved untouched
            (item_a, b, item_b)
        } else {
            let _ = timer_a;
            let _ = timer_b;
            (item_b, a, item_a)
        };

        let max_stack = registry
            .get(keeper_item.stack.item)
            .map(|def| def.max_stack.get())
            .unwrap_or(u16::MAX);

        let combined = keeper_item
            .stack
            .count
            .get()
            .saturating_add(donor_item.stack.count.get());

        if combined <= max_stack {
            keeper_item.stack = ItemStack::new(
                keeper_item.stack.item,
                std::num::NonZero::new(combined).expect("combined >= 1"),
            );
            commands.entity(donor_entity).despawn();
        } else {
            let spillover = combined - max_stack;
            keeper_item.stack = ItemStack::new(
                keeper_item.stack.item,
                std::num::NonZero::new(max_stack).expect("max_stack > 0"),
            );
            donor_item.stack = ItemStack::new(
                donor_item.stack.item,
                std::num::NonZero::new(spillover)
                    .expect("spillover > 0 because combined > max_stack"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::{Timer, TimerMode};
    use dd40_item_core::plugin::ItemCorePlugin;
    use dd40_item_core::registry::{ItemDefinition, ItemId};
    use dd40_loose_item_core::plugin::LooseItemCorePlugin;
    use dd40_loose_item_core::system_sets::LooseItemSet;
    use dd40_physics_core::plugin::PhysicsCorePlugin;
    use std::num::NonZero;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("non-zero literal")
    }

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            ItemCorePlugin,
            PhysicsCorePlugin,
            LooseItemCorePlugin,
        ))
        .init_resource::<MergeAccumulator>()
        .add_systems(
            Update,
            accumulate_and_merge_loose_items.in_set(LooseItemSet::Resolve),
        );
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::MAX);
        app
    }

    fn register_stone(app: &mut App, max_stack: u16) -> ItemId {
        let mut registry = app.world_mut().resource_mut::<ItemRegistry>();
        registry
            .register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(max_stack)))
    }

    fn spawn_loose(app: &mut App, item: ItemId, count: u16, lifetime_secs: u64) -> Entity {
        app.world_mut()
            .spawn((
                LooseItem::new(ItemStack::new(item, nz(count))),
                DespawnTimer(Timer::new(
                    Duration::from_secs(lifetime_secs),
                    TimerMode::Once,
                )),
            ))
            .id()
    }

    fn write_contact(app: &mut App, a: Entity, b: Entity) {
        app.world_mut()
            .write_message(BodyBodyContact::new(a, b, Vec3::Y, 0.0));
    }

    fn canonical(a: Entity, b: Entity) -> (Entity, Entity) {
        super::canonical_pair(a, b)
    }

    #[test]
    fn accumulator_grows_while_pair_is_in_contact() {
        let mut app = new_app();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(100),
        ));
        let id = register_stone(&mut app, 64);
        let a = spawn_loose(&mut app, id, 1, 60);
        let b = spawn_loose(&mut app, id, 1, 60);

        for _ in 0..4 {
            write_contact(&mut app, a, b);
            app.update();
        }

        let elapsed = app
            .world()
            .resource::<MergeAccumulator>()
            .elapsed(a, b)
            .unwrap();
        // Bevy's first Update has delta = 0, so 4 ticks at 100 ms each
        // yields 300 ms accumulated.
        assert!(
            elapsed >= Duration::from_millis(200),
            "elapsed={:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "should not yet have merged: elapsed={:?}",
            elapsed
        );
    }

    #[test]
    fn accumulator_drops_pair_after_one_tick_without_contact() {
        let mut app = new_app();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(100),
        ));
        let id = register_stone(&mut app, 64);
        let a = spawn_loose(&mut app, id, 1, 60);
        let b = spawn_loose(&mut app, id, 1, 60);

        write_contact(&mut app, a, b);
        app.update();
        assert!(
            app.world()
                .resource::<MergeAccumulator>()
                .elapsed(a, b)
                .is_some()
        );

        // No contact this tick → entry should be cleared.
        app.update();
        assert!(
            app.world()
                .resource::<MergeAccumulator>()
                .elapsed(a, b)
                .is_none()
        );
    }

    #[test]
    fn pair_merges_after_threshold_and_keeps_larger_timer() {
        let mut app = new_app();
        // 250 ms steps so we hit the 1 s default in 4 ticks.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(250),
        ));
        let id = register_stone(&mut app, 64);
        // Larger stack gets the longer lifetime.
        let large = spawn_loose(&mut app, id, 10, 200);
        let small = spawn_loose(&mut app, id, 3, 30);

        for _ in 0..5 {
            write_contact(&mut app, large, small);
            app.update();
        }

        // Donor (smaller) despawned.
        assert!(app.world().get::<LooseItem>(small).is_none());
        // Keeper (larger) now holds the combined stack.
        let keeper = app.world().get::<LooseItem>(large).unwrap();
        assert_eq!(keeper.stack.count.get(), 13);
        // Keeper's DespawnTimer is the original 200 s timer.
        let timer = app.world().get::<DespawnTimer>(large).unwrap();
        assert_eq!(timer.0.duration(), Duration::from_secs(200));
    }

    #[test]
    fn merge_spills_excess_back_onto_donor() {
        let mut app = new_app();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(250),
        ));
        let id = register_stone(&mut app, 64);
        let big = spawn_loose(&mut app, id, 60, 200);
        let med = spawn_loose(&mut app, id, 30, 30);

        for _ in 0..5 {
            write_contact(&mut app, big, med);
            app.update();
        }

        // 60 + 30 = 90; max_stack = 64; spillover = 26.
        assert_eq!(
            app.world().get::<LooseItem>(big).unwrap().stack.count.get(),
            64
        );
        assert_eq!(
            app.world().get::<LooseItem>(med).unwrap().stack.count.get(),
            26
        );
    }

    #[test]
    fn different_item_types_never_merge() {
        let mut app = new_app();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(250),
        ));
        let stone = register_stone(&mut app, 64);
        let wood = {
            let mut registry = app.world_mut().resource_mut::<ItemRegistry>();
            registry.register_auto(ItemDefinition::new(ItemId(0), "wood").with_max_stack(nz(64)))
        };
        let a = spawn_loose(&mut app, stone, 1, 30);
        let b = spawn_loose(&mut app, wood, 1, 30);

        for _ in 0..5 {
            write_contact(&mut app, a, b);
            app.update();
        }

        assert!(app.world().get::<LooseItem>(a).is_some());
        assert!(app.world().get::<LooseItem>(b).is_some());
        // Pair was never even tracked.
        assert!(
            app.world()
                .resource::<MergeAccumulator>()
                .elapsed(a, b)
                .is_none()
        );
    }

    #[test]
    fn equal_counts_keep_lower_index_as_keeper() {
        let mut app = new_app();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(250),
        ));
        let id = register_stone(&mut app, 64);
        let a = spawn_loose(&mut app, id, 5, 30);
        let b = spawn_loose(&mut app, id, 5, 30);

        for _ in 0..5 {
            write_contact(&mut app, a, b);
            app.update();
        }

        let (low, high) = canonical(a, b);
        assert!(
            app.world().get::<LooseItem>(low).is_some(),
            "lower-index entity must survive"
        );
        assert!(
            app.world().get::<LooseItem>(high).is_none(),
            "higher-index entity must be despawned"
        );
        assert_eq!(
            app.world().get::<LooseItem>(low).unwrap().stack.count.get(),
            10
        );
    }
}
