//! [`InventoryComponent`] — entity-attached wrapper around
//! [`Inventory`].
//!
//! Wraps a pure-data [`Inventory`] in a Bevy [`Component`] and adds
//! event-firing variants of every mutator: each successful mutation
//! triggers an [`InventoryChanged`] event carrying the holder
//! [`Entity`] and the per-slot diff produced by the underlying
//! [`Inventory`].
//!
//! # When to use this vs `BlockInventory`
//!
//! - Use [`InventoryComponent`] for inventories that live on an entity
//!   (characters, mobs, dropped item entities, vehicles…).
//! - Use [`BlockInventory`](crate::block::BlockInventory) for
//!   inventories that live in a specific block cell (chests, hoppers,
//!   furnaces…).
//!
//! Both share the same [`Inventory`] implementation, so item-flow
//! logic written against `&mut Inventory` works against either
//! container.
//!
//! # Direct data access
//!
//! `InventoryComponent::inventory` / `inventory_mut` expose the
//! underlying `Inventory`.  Mutating via `inventory_mut` skips event
//! emission — useful for pre-spawn population and batch operations
//! where the caller wants to emit one summary event of its own.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;

use crate::inventory::{InsertError, Inventory, SlotChange};

/// Entity-targeted event triggered after every successful mutation of
/// an [`InventoryComponent`].
///
/// Triggered via [`Commands::trigger`]; observers reach the holder via
/// `trigger.entity`.
///
/// # Batching and ordering
///
/// `changes` contains exactly **one entry per slot the call modified**
/// — duplicate slot entries never appear.  The order of entries within
/// `changes` is unspecified; callers that need ordered output should
/// sort by `slot`.
///
/// No-op calls (failed strict insert, take from an empty slot, set to
/// identical contents, …) fire no event.  "Event observed" is therefore
/// a reliable signal that inventory contents actually moved.
#[derive(EntityEvent, Debug, Clone)]
pub struct InventoryChanged {
    /// The inventory entity this event targets.
    pub entity: Entity,
    /// Per-slot diff for the call that triggered this event.
    pub changes: Vec<SlotChange>,
}

/// Entity-attached [`Inventory`] wrapper.
///
/// See the [module-level docs](self) for the event semantics.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct InventoryComponent {
    inventory: Inventory,
}

impl InventoryComponent {
    /// Creates a component wrapping an empty inventory of the given
    /// capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inventory: Inventory::with_capacity(capacity),
        }
    }

    /// Creates a component wrapping a pre-populated [`Inventory`].
    pub fn from_inventory(inventory: Inventory) -> Self {
        Self { inventory }
    }

    /// Returns a reference to the underlying [`Inventory`].
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Returns a mutable reference to the underlying [`Inventory`].
    ///
    /// **No events fire** while mutating through this handle.  Useful
    /// for pre-spawn population and tests; for player-facing mutations
    /// use the dedicated event-firing methods below.
    pub fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.inventory
    }

    // ─── Event-firing mutators ───────────────────────────────────────────

    /// Auto-merging insert that fires an [`InventoryChanged`] event on
    /// `entity` describing every modified slot.
    ///
    /// Returns the leftover stack the inventory could not absorb.  No
    /// event fires when the call is a no-op.
    pub fn insert_stack(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (leftover, changes) = self.inventory.insert_stack(stack, registry);
        emit_if_nonempty(commands, entity, changes);
        leftover
    }

    /// Per-slot insert that fires an [`InventoryChanged`] event on
    /// `entity` carrying a single [`SlotChange`] on success.  Fires no
    /// event on error.
    pub fn insert_stack_strict(
        &mut self,
        slot: usize,
        stack: ItemStack,
        commands: &mut Commands,
        entity: Entity,
    ) -> Result<(), InsertError> {
        let change = self.inventory.insert_stack_strict(slot, stack)?;
        emit_if_nonempty(commands, entity, vec![change]);
        Ok(())
    }

    /// Removes the stack in `slot`, firing an [`InventoryChanged`]
    /// event on `entity` when something was actually removed.
    pub fn take_slot(
        &mut self,
        slot: usize,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (taken, changes) = self.inventory.take_slot(slot);
        emit_if_nonempty(commands, entity, changes);
        taken
    }

    /// Removes up to `n` items from `slot`, firing an
    /// [`InventoryChanged`] event on `entity` when something was
    /// actually removed.
    pub fn take_slot_n(
        &mut self,
        slot: usize,
        n: u16,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (taken, changes) = self.inventory.take_slot_n(slot, n);
        emit_if_nonempty(commands, entity, changes);
        taken
    }

    /// Replaces the contents of `slot`, firing an [`InventoryChanged`]
    /// event on `entity` when the contents actually changed.
    pub fn set_slot(
        &mut self,
        slot: usize,
        stack: Option<ItemStack>,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (previous, changes) = self.inventory.set_slot(slot, stack);
        emit_if_nonempty(commands, entity, changes);
        previous
    }

    /// Sets the [`Inventory::active_slot`].
    ///
    /// No [`InventoryChanged`] event fires (no slot contents move),
    /// but the underlying [`InventoryComponent`] is mutably touched
    /// so Bevy `Changed<InventoryComponent>` filters fire — which is
    /// what the active-item refresh system observes.
    ///
    /// Out-of-range values are clamped (see
    /// [`Inventory::set_active_slot`]).
    pub fn set_active_slot(&mut self, slot: u8) {
        self.inventory.set_active_slot(slot);
    }

    /// Removes up to `count` items from the currently-active slot,
    /// firing an [`InventoryChanged`] event on `entity` when something
    /// was actually removed.
    pub fn decrement_active_slot(
        &mut self,
        count: u16,
        commands: &mut Commands,
        entity: Entity,
    ) -> Option<ItemStack> {
        let (taken, changes) = self.inventory.decrement_active_slot(count);
        emit_if_nonempty(commands, entity, changes);
        taken
    }
}

fn emit_if_nonempty(commands: &mut Commands, entity: Entity, changes: Vec<SlotChange>) {
    if changes.is_empty() {
        return;
    }
    commands.trigger(InventoryChanged { entity, changes });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry};
    use std::num::NonZero;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("nz literal must be non-zero")
    }

    fn registry_with_basics() -> ItemRegistry {
        let mut reg = ItemRegistry::new();
        reg.register(ItemDefinition::new(ItemId(1), "stone").with_max_stack(nz(64)));
        reg.register(ItemDefinition::new(ItemId(2), "tool").with_max_stack(nz(1)));
        reg
    }

    #[derive(Resource, Default)]
    struct Captured(Vec<InventoryChanged>);

    fn capture_observer(trigger: On<InventoryChanged>, mut captured: ResMut<Captured>) {
        captured.0.push(InventoryChanged {
            entity: trigger.entity,
            changes: trigger.changes.clone(),
        });
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.init_resource::<Captured>();
        app.add_observer(capture_observer);
        app
    }

    #[test]
    fn insert_stack_fires_one_event_with_one_change_per_modified_slot() {
        let mut app = make_app();
        let registry = registry_with_basics();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(4))
            .id();
        app.world_mut()
            .get_mut::<InventoryComponent>(entity)
            .unwrap()
            .inventory_mut()
            .set_slot(0, Some(ItemStack::new(ItemId(1), nz(60))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut InventoryComponent>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    inv.insert_stack(
                        ItemStack::new(ItemId(1), nz(70)),
                        &registry,
                        &mut commands,
                        entity,
                    );
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1);
        let mut changes = captured.0[0].changes.clone();
        assert_eq!(changes.len(), 3);
        changes.sort_by_key(|c| c.slot);
        assert_eq!(changes[0].slot, 0);
        assert_eq!(changes[1].slot, 1);
        assert_eq!(changes[2].slot, 2);
    }

    #[test]
    fn insert_strict_failure_fires_no_event() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(2))
            .id();
        app.world_mut()
            .get_mut::<InventoryComponent>(entity)
            .unwrap()
            .inventory_mut()
            .set_slot(0, Some(ItemStack::single(ItemId(1))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut InventoryComponent>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let res = inv.insert_stack_strict(
                        0,
                        ItemStack::single(ItemId(2)),
                        &mut commands,
                        entity,
                    );
                    assert!(res.is_err());
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn take_slot_on_empty_fires_no_event() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(2))
            .id();

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut InventoryComponent>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let taken = inv.take_slot(0, &mut commands, entity);
                    assert!(taken.is_none());
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn set_slot_replacing_fires_event_with_correct_previous_and_current() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(2))
            .id();
        app.world_mut()
            .get_mut::<InventoryComponent>(entity)
            .unwrap()
            .inventory_mut()
            .set_slot(0, Some(ItemStack::new(ItemId(1), nz(5))));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut InventoryComponent>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    let prev = inv.set_slot(
                        0,
                        Some(ItemStack::new(ItemId(2), nz(1))),
                        &mut commands,
                        entity,
                    );
                    assert_eq!(prev.unwrap().item, ItemId(1));
                },
            )
            .unwrap();

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1);
        let change = &captured.0[0].changes[0];
        assert_eq!(change.slot, 0);
        assert_eq!(change.previous.as_ref().unwrap().item, ItemId(1));
        assert_eq!(change.current.as_ref().unwrap().item, ItemId(2));
    }

    #[test]
    fn set_slot_to_identical_fires_no_event() {
        let mut app = make_app();
        let entity = app
            .world_mut()
            .spawn(InventoryComponent::with_capacity(1))
            .id();
        let stack = ItemStack::new(ItemId(1), nz(3));
        app.world_mut()
            .get_mut::<InventoryComponent>(entity)
            .unwrap()
            .inventory_mut()
            .set_slot(0, Some(stack));

        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut q: Query<&mut InventoryComponent>| {
                    let mut inv = q.get_mut(entity).unwrap();
                    inv.set_slot(0, Some(stack), &mut commands, entity);
                },
            )
            .unwrap();

        assert!(app.world().resource::<Captured>().0.is_empty());
    }
}
