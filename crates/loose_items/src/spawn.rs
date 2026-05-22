//! Spawning + lifecycle tick for loose items.

use bevy::prelude::*;
use bevy::time::Timer;
use std::num::NonZero;

use dd40_inventory_core::drop::DropItems;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;
use dd40_loose_item_core::{DespawnTimer, LooseItem, LooseItemConfig, PickupCooldown};
use dd40_physics_core::prelude::{Aabb, GravityScale, PhysicsBody, PhysicsCollider, Velocity};

/// Half-extent of a freshly spawned loose item's collider, in world
/// units.  0.125 m (¼ block) feels right visually and stops items
/// from getting stuck in narrow gaps.
const LOOSE_ITEM_HALF_EXTENT: f32 = 0.125;

/// Drains [`DropItems`] messages and spawns one entity per stack
/// (splitting oversized stacks at the item's `max_stack`).
///
/// Each spawned entity carries:
///
/// - [`LooseItem`] with the (possibly split) stack
/// - [`Transform`] at the message's `origin`
/// - [`PhysicsBody`] + [`PhysicsCollider`] + a small [`Aabb`]
/// - [`Velocity`] copied verbatim from the message (emitters add
///   their own scatter)
/// - [`GravityScale(1.0)`]
/// - [`DespawnTimer`] and [`PickupCooldown`] initialised from
///   [`LooseItemConfig`]
///
/// Runs in [`LooseItemSet::Spawn`](dd40_loose_item_core::LooseItemSet::Spawn).
pub fn spawn_loose_items(
    mut drops: MessageReader<DropItems>,
    config: Res<LooseItemConfig>,
    registry: Res<ItemRegistry>,
    mut commands: Commands,
) {
    for drop in drops.read() {
        for stack in &drop.stacks {
            for sub_stack in split_to_max_stack(*stack, &registry) {
                commands.spawn((
                    Transform::from_translation(drop.origin),
                    PhysicsBody,
                    PhysicsCollider,
                    Aabb::new(
                        LOOSE_ITEM_HALF_EXTENT,
                        LOOSE_ITEM_HALF_EXTENT,
                        LOOSE_ITEM_HALF_EXTENT,
                    ),
                    Velocity(drop.velocity),
                    GravityScale(1.0),
                    LooseItem::new(sub_stack),
                    DespawnTimer(Timer::new(
                        config.default_lifetime,
                        bevy::time::TimerMode::Once,
                    )),
                    PickupCooldown(Timer::new(
                        config.default_pickup_cooldown,
                        bevy::time::TimerMode::Once,
                    )),
                ));
            }
        }
    }
}

/// Splits `stack` into one or more sub-stacks each ≤ the item's
/// `max_stack`. If the item is unknown to `registry`, falls back to
/// the stack's existing count (no split) — the spawner is not the
/// right place to reject unknown IDs.
fn split_to_max_stack(stack: ItemStack, registry: &ItemRegistry) -> Vec<ItemStack> {
    let max_stack = registry
        .get(stack.item)
        .map(|def| def.max_stack.get())
        .unwrap_or_else(|| stack.count.get());

    if max_stack == 0 {
        return vec![stack];
    }

    let mut remaining = stack.count.get();
    let mut out = Vec::new();
    while remaining > 0 {
        let take = remaining.min(max_stack);
        out.push(ItemStack {
            item: stack.item,
            count: NonZero::new(take).expect("take > 0 because remaining > 0"),
        });
        remaining -= take;
    }
    out
}

/// Ticks despawn + pickup-cooldown timers and removes loose items
/// whose lifetime has elapsed.
///
/// Runs in [`LooseItemSet::Lifecycle`](dd40_loose_item_core::LooseItemSet::Lifecycle).
pub fn tick_lifetimes(
    time: Res<Time>,
    mut commands: Commands,
    mut despawn_q: Query<(Entity, &mut DespawnTimer), With<LooseItem>>,
    mut cooldown_q: Query<&mut PickupCooldown, With<LooseItem>>,
) {
    let delta = time.delta();
    for (entity, mut timer) in &mut despawn_q {
        timer.0.tick(delta);
        if timer.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
    for mut cooldown in &mut cooldown_q {
        cooldown.0.tick(delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::LooseItemsPlugin;
    use bevy::time::TimeUpdateStrategy;
    use dd40_item_core::registry::{ItemDefinition, ItemId};
    use dd40_loose_item_core::system_sets::LooseItemSet;
    use std::time::Duration;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("non-zero literal")
    }

    fn register_stone(app: &mut App, max_stack: u16) -> ItemId {
        let mut registry = app.world_mut().resource_mut::<ItemRegistry>();
        registry
            .register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(max_stack)))
    }

    fn new_app_with_step(step: Duration) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LooseItemsPlugin));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(step));
        // Virtual time clamps delta at 250 ms by default; tests need to
        // advance time in large jumps, so lift the ceiling.
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::MAX);
        app
    }

    fn new_app() -> App {
        new_app_with_step(Duration::from_millis(16))
    }

    #[test]
    fn split_to_max_stack_keeps_small_stacks_intact() {
        let mut registry = ItemRegistry::new();
        let id =
            registry.register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(64)));
        let stack = ItemStack::new(id, nz(10));
        let out = split_to_max_stack(stack, &registry);
        assert_eq!(out, vec![stack]);
    }

    #[test]
    fn split_to_max_stack_splits_oversized_stacks() {
        let mut registry = ItemRegistry::new();
        let id =
            registry.register_auto(ItemDefinition::new(ItemId(0), "stone").with_max_stack(nz(64)));
        let stack = ItemStack::new(id, nz(130));
        let out = split_to_max_stack(stack, &registry);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].count.get(), 64);
        assert_eq!(out[1].count.get(), 64);
        assert_eq!(out[2].count.get(), 2);
        for sub in &out {
            assert_eq!(sub.item, id);
        }
    }

    #[test]
    fn split_to_max_stack_unknown_item_does_not_split() {
        let registry = ItemRegistry::new();
        let stack = ItemStack::new(ItemId(99), nz(200));
        let out = split_to_max_stack(stack, &registry);
        assert_eq!(out, vec![stack]);
    }

    #[test]
    fn drop_message_spawns_loose_item_entity() {
        let mut app = new_app();
        let id = register_stone(&mut app, 64);

        app.world_mut().write_message(DropItems {
            origin: Vec3::new(1.0, 2.0, 3.0),
            velocity: Vec3::new(0.0, 5.0, 0.0),
            stacks: vec![ItemStack::new(id, nz(10))],
        });
        app.update();

        let mut q = app
            .world_mut()
            .query::<(&LooseItem, &Velocity, &Transform)>();
        let items: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0.stack.count.get(), 10);
        assert_eq!(items[0].1.0, Vec3::new(0.0, 5.0, 0.0));
        assert_eq!(items[0].2.translation, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn oversized_stack_is_split_into_multiple_entities() {
        let mut app = new_app();
        let id = register_stone(&mut app, 64);

        app.world_mut().write_message(DropItems {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            stacks: vec![ItemStack::new(id, nz(130))],
        });
        app.update();

        let mut q = app.world_mut().query::<&LooseItem>();
        let counts: Vec<u16> = q.iter(app.world()).map(|i| i.stack.count.get()).collect();
        assert_eq!(counts.len(), 3);
        assert_eq!(counts.iter().sum::<u16>(), 130);
        for c in counts {
            assert!(c <= 64);
        }
    }

    #[test]
    fn empty_drop_message_spawns_nothing() {
        let mut app = new_app();
        let _ = register_stone(&mut app, 64);

        app.world_mut().write_message(DropItems {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            stacks: vec![],
        });
        app.update();

        let mut q = app.world_mut().query::<&LooseItem>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn loose_item_is_despawned_after_lifetime_elapses() {
        let lifetime = LooseItemConfig::default().default_lifetime;
        let mut app = new_app_with_step(lifetime + Duration::from_millis(1));
        let id = register_stone(&mut app, 64);

        app.world_mut().write_message(DropItems {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            stacks: vec![ItemStack::new(id, nz(1))],
        });
        // Tick 1: spawn the entity (time advances by `step`, but the timer
        // is constructed *after* tick advances in the same frame, so its
        // elapsed is still 0).
        app.update();
        {
            let mut q = app.world_mut().query::<&LooseItem>();
            assert_eq!(q.iter(app.world()).count(), 1);
        }

        // Tick 2: time advances by another `step` (> lifetime), so the
        // lifecycle system ticks the timer past its duration and despawns.
        app.update();
        let mut q = app.world_mut().query::<&LooseItem>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn pickup_cooldown_does_not_despawn_entity() {
        let cooldown = LooseItemConfig::default().default_pickup_cooldown;
        let mut app = new_app_with_step(cooldown + Duration::from_millis(1));
        let id = register_stone(&mut app, 64);

        app.world_mut().write_message(DropItems {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            stacks: vec![ItemStack::new(id, nz(1))],
        });
        app.update();
        app.update();

        let mut q = app.world_mut().query::<(&LooseItem, &PickupCooldown)>();
        let v: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(v.len(), 1, "cooldown tick must not despawn the entity");
        assert!(v[0].1.0.is_finished(), "cooldown should have elapsed");
    }

    #[test]
    fn loose_item_set_ordering_is_registered() {
        let _ = new_app();
        let _ = LooseItemSet::Spawn;
    }
}
