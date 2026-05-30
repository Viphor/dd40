//! [`InventorySlotProvider`] — the [`ActiveItemProvider`] backed by an
//! [`InventoryComponent`].
//!
//! The provider reads / mutates `InventoryComponent` on a single
//! "inventory holder" entity, at whichever slot the inventory's
//! `active_slot()` happens to be.  Concretely:
//!
//! - [`current`][ActiveItemProvider::current] returns the stack in
//!   `inv.slot(inv.active_slot() as usize)`.
//! - [`consume`][ActiveItemProvider::consume] calls
//!   [`InventoryComponent::decrement_active_slot`], which fires the
//!   standard `InventoryChanged` event so any downstream consumer
//!   (GUI, networking, audio) reacts the same way it would to any
//!   other inventory mutation.
//!
//! # Wiring
//!
//! [`VanillaInventoryActiveItemPlugin`] handles attachment:
//!
//! - On `Added<InventoryComponent>` with no existing `ActiveItem`,
//!   inserts `ActiveItem::with_provider(InventorySlotProvider { ... })`
//!   pointing at the same entity.  No `Player` filter — server-side
//!   `NetworkCharacter`s and client-side remote characters get the
//!   component too, so any future system can read "what is this
//!   character holding".
//! - On `Changed<InventoryComponent>`, runs
//!   [`refresh_active_item_cache`] (exclusive) to pull a fresh
//!   snapshot into [`ActiveItem`]'s cache.
//!
//! Read-side gameplay code stays unchanged: it sees the cached
//! [`ItemStack`] via [`ActiveItem::peek`] /
//! [`ActiveItem::item`] without ever touching this provider.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_inventory_core::component::InventoryComponent;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::active_item::{ActiveItem, ActiveItemProvider, ItemStack};
use dd40_item_core::plugin::ItemCorePlugin;

/// [`ActiveItemProvider`] that reads / mutates `InventoryComponent`
/// on `inventory` at whichever slot is currently active.
#[derive(Debug, Clone, Copy)]
pub struct InventorySlotProvider {
    /// The entity carrying the [`InventoryComponent`] this provider
    /// reads from.  Almost always the same entity the [`ActiveItem`]
    /// is attached to.
    pub inventory: Entity,
}

impl ActiveItemProvider for InventorySlotProvider {
    fn current(&self, world: &World) -> Option<ItemStack> {
        let inv = world.get::<InventoryComponent>(self.inventory)?;
        let slot = inv.inventory().active_slot() as usize;
        inv.inventory().slot(slot).copied()
    }

    fn consume(&mut self, world: &mut World, count: u16) -> u16 {
        if count == 0 {
            return 0;
        }
        let entity = self.inventory;
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            return 0;
        };
        let Some(mut inv) = entity_mut.get_mut::<InventoryComponent>() else {
            return 0;
        };
        // Mutate via the underlying `Inventory` so we don't need a
        // `Commands` (which would conflict with the `&mut World`
        // borrow we already hold).  We trigger the equivalent
        // `InventoryChanged` event ourselves once the borrow is gone.
        let (taken, changes) = inv.inventory_mut().decrement_active_slot(count);
        drop(entity_mut);
        if !changes.is_empty() {
            world.trigger(dd40_inventory_core::component::InventoryChanged { entity, changes });
        }
        taken.map(|s| s.count.get()).unwrap_or(0)
    }
}

/// Plugin that attaches [`ActiveItem`] to every entity that gains an
/// [`InventoryComponent`] and keeps the cached snapshot in sync.
///
/// Add this on **both** the client and the server in a networked
/// build — gameplay logic on both sides needs to read "what is this
/// character holding".
#[derive(Default)]
pub struct VanillaInventoryActiveItemPlugin;

impl Plugin for VanillaInventoryActiveItemPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, InventoryCorePlugin, ItemCorePlugin);
        app.add_systems(
            Update,
            (attach_active_item, refresh_active_item_cache).chain(),
        );
    }
}

/// Inserts an [`ActiveItem`] (with an [`InventorySlotProvider`]) on
/// every newly-added [`InventoryComponent`] that does not already
/// carry one.
pub fn attach_active_item(
    mut commands: Commands,
    new_inventories: Query<Entity, (Added<InventoryComponent>, Without<ActiveItem>)>,
) {
    for entity in &new_inventories {
        commands
            .entity(entity)
            .insert(ActiveItem::with_provider(InventorySlotProvider {
                inventory: entity,
            }));
    }
}

/// Refreshes the cache on every [`ActiveItem`] whose backing
/// [`InventoryComponent`] changed this tick (slot edited, active slot
/// switched, etc.).
///
/// Exclusive system: the provider's [`current`][ActiveItemProvider::current]
/// takes `&World`, which conflicts with holding a `&mut ActiveItem`
/// out of an ECS query.  We resolve that by `take` + refresh +
/// `insert` against the world directly.
pub fn refresh_active_item_cache(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (With<ActiveItem>, Changed<InventoryComponent>)>();
    let entities: Vec<Entity> = q.iter(world).collect();
    for entity in entities {
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        let Some(mut active) = entity_mut.take::<ActiveItem>() else {
            continue;
        };
        active.refresh(world);
        world.entity_mut(entity).insert(active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_item_core::registry::ItemId;
    use std::num::NonZero;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("non-zero literal")
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(VanillaInventoryActiveItemPlugin);
        app
    }

    #[test]
    fn attaches_active_item_to_new_inventory() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(9))
            .id();
        app.update();
        assert!(
            app.world().get::<ActiveItem>(entity).is_some(),
            "ActiveItem must be auto-attached on Added<InventoryComponent>"
        );
    }

    #[test]
    fn refresh_pulls_active_slot_into_cache() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(9))
            .id();
        app.update();
        // Put an item in slot 3, then set active_slot = 3.
        {
            let mut inv = app
                .world_mut()
                .get_mut::<InventoryComponent>(entity)
                .unwrap();
            inv.inventory_mut()
                .set_slot(3, Some(ItemStack::new(ItemId(42), nz(7))));
            inv.set_active_slot(3);
        }
        app.update();
        let active = app.world().get::<ActiveItem>(entity).unwrap();
        let stack = active.peek().expect("cache filled after refresh");
        assert_eq!(stack.item, ItemId(42));
        assert_eq!(stack.count.get(), 7);
    }

    #[test]
    fn consume_decrements_underlying_slot() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(9))
            .id();
        app.update();
        {
            let mut inv = app
                .world_mut()
                .get_mut::<InventoryComponent>(entity)
                .unwrap();
            inv.inventory_mut()
                .set_slot(0, Some(ItemStack::new(ItemId(1), nz(3))));
        }
        app.update();
        // Consume via take+insert pattern (mirrors the placement code path).
        {
            let world = app.world_mut();
            let mut active = world.entity_mut(entity).take::<ActiveItem>().unwrap();
            let taken = active.consume(world, 1);
            assert_eq!(taken, 1);
            world.entity_mut(entity).insert(active);
        }
        // After consume, underlying slot should show 2.
        let inv = app.world().get::<InventoryComponent>(entity).unwrap();
        assert_eq!(inv.inventory().slot(0).unwrap().count.get(), 2);
    }
}
