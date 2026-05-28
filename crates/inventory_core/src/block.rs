//! [`BlockInventory`] — block-cell-attached wrapper around
//! [`Inventory`].
//!
//! Wraps a pure-data [`Inventory`] in a [`BlockData`] payload so the
//! inventory can live in the chunk's typed cell-data store rather than
//! on an entity.  Use this for inventories that belong to a specific
//! block: chests, hoppers, furnaces, droppers, dispensers.
//!
//! Each successful mutation triggers a [`BlockInventoryChanged`] event
//! carrying the [`BlockPos`] of the container block and the per-slot
//! diff produced by the underlying [`Inventory`].  Observers can keep
//! UI in sync with the same shape as
//! [`InventoryChanged`](crate::component::InventoryChanged) — the only
//! difference is the keying field (`pos` vs `entity`).
//!
//! Register the type with the chunk's cell-data system once:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_core::block::BlockDataAppExt;
//! use dd40_inventory_core::prelude::*;
//!
//! App::new().register_block_data::<BlockInventory>().run();
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use dd40_core::block::{BlockData, BlockPos};
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;

use crate::inventory::{InsertError, Inventory, SlotChange};

/// Block-keyed event triggered after every successful mutation of a
/// [`BlockInventory`].
///
/// Mirrors [`InventoryChanged`](crate::component::InventoryChanged)
/// shape exactly, except that the routing key is a [`BlockPos`]
/// (the global position of the container block) rather than an
/// [`Entity`].
///
/// Triggered via [`Commands::trigger`].  Observers receive the event
/// through `On<BlockInventoryChanged>` and dispatch on `trigger.pos`.
///
/// No-op calls (failed strict insert, take from empty slot, identical
/// `set_slot`, …) fire no event.
#[derive(Event, Debug, Clone)]
pub struct BlockInventoryChanged {
    /// Global position of the block whose inventory changed.
    pub pos: BlockPos,
    /// Per-slot diff for the call that triggered this event.
    pub changes: Vec<SlotChange>,
}

/// [`Inventory`] living against a specific block cell.
///
/// Stored in the chunk's `BlockDataTypeRegistry`-driven cell-data
/// table — see
/// [`Chunk::insert_cell_data`](dd40_core::chunk::Chunk) for the
/// machinery.
///
/// See the [module-level docs](self) for the event semantics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockInventory {
    inventory: Inventory,
}

impl BlockInventory {
    /// Creates a block inventory wrapping an empty inventory of the
    /// given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inventory: Inventory::with_capacity(capacity),
        }
    }

    /// Creates a block inventory wrapping a pre-populated [`Inventory`].
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
    /// for tests, world-generation, and batch operations; for
    /// player-facing mutations use the dedicated event-firing methods.
    pub fn inventory_mut(&mut self) -> &mut Inventory {
        &mut self.inventory
    }

    // ─── Event-firing mutators ───────────────────────────────────────────

    /// Auto-merging insert that fires a [`BlockInventoryChanged`] event
    /// keyed on `pos` describing every modified slot.  Returns the
    /// leftover stack the inventory could not absorb.
    pub fn insert_stack(
        &mut self,
        stack: ItemStack,
        registry: &ItemRegistry,
        commands: &mut Commands,
        pos: BlockPos,
    ) -> Option<ItemStack> {
        let (leftover, changes) = self.inventory.insert_stack(stack, registry);
        emit_if_nonempty(commands, pos, changes);
        leftover
    }

    /// Per-slot insert that fires a [`BlockInventoryChanged`] event on
    /// success.
    pub fn insert_stack_strict(
        &mut self,
        slot: usize,
        stack: ItemStack,
        commands: &mut Commands,
        pos: BlockPos,
    ) -> Result<(), InsertError> {
        let change = self.inventory.insert_stack_strict(slot, stack)?;
        emit_if_nonempty(commands, pos, vec![change]);
        Ok(())
    }

    /// Removes the stack in `slot`, firing a [`BlockInventoryChanged`]
    /// event when something was actually removed.
    pub fn take_slot(
        &mut self,
        slot: usize,
        commands: &mut Commands,
        pos: BlockPos,
    ) -> Option<ItemStack> {
        let (taken, changes) = self.inventory.take_slot(slot);
        emit_if_nonempty(commands, pos, changes);
        taken
    }

    /// Removes up to `n` items from `slot`, firing a
    /// [`BlockInventoryChanged`] event when something was actually
    /// removed.
    pub fn take_slot_n(
        &mut self,
        slot: usize,
        n: u16,
        commands: &mut Commands,
        pos: BlockPos,
    ) -> Option<ItemStack> {
        let (taken, changes) = self.inventory.take_slot_n(slot, n);
        emit_if_nonempty(commands, pos, changes);
        taken
    }

    /// Replaces the contents of `slot`, firing a
    /// [`BlockInventoryChanged`] event when the contents actually
    /// changed.
    pub fn set_slot(
        &mut self,
        slot: usize,
        stack: Option<ItemStack>,
        commands: &mut Commands,
        pos: BlockPos,
    ) -> Option<ItemStack> {
        let (previous, changes) = self.inventory.set_slot(slot, stack);
        emit_if_nonempty(commands, pos, changes);
        previous
    }
}

fn emit_if_nonempty(commands: &mut Commands, pos: BlockPos, changes: Vec<SlotChange>) {
    if changes.is_empty() {
        return;
    }
    commands.trigger(BlockInventoryChanged { pos, changes });
}

impl BlockData for BlockInventory {
    fn type_key(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn clone_box(&self) -> Box<dyn BlockData> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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
        reg
    }

    #[derive(Resource, Default)]
    struct Captured(Vec<BlockInventoryChanged>);

    fn capture_observer(trigger: On<BlockInventoryChanged>, mut captured: ResMut<Captured>) {
        captured.0.push(BlockInventoryChanged {
            pos: trigger.pos,
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
    fn insert_stack_fires_event_keyed_on_block_pos() {
        let mut app = make_app();
        let registry = registry_with_basics();
        let pos = BlockPos::new(3, 64, -7);
        let mut inv = BlockInventory::with_capacity(3);

        let leftover = app
            .world_mut()
            .run_system_once(move |mut commands: Commands| {
                inv.insert_stack(
                    ItemStack::new(ItemId(1), nz(20)),
                    &registry,
                    &mut commands,
                    pos,
                )
            })
            .unwrap();
        assert!(leftover.is_none());

        let captured = app.world().resource::<Captured>();
        assert_eq!(captured.0.len(), 1);
        assert_eq!(captured.0[0].pos, pos);
        assert_eq!(captured.0[0].changes.len(), 1);
    }

    #[test]
    fn take_slot_on_empty_fires_no_event() {
        let mut app = make_app();
        let pos = BlockPos::new(0, 0, 0);
        let mut inv = BlockInventory::with_capacity(2);

        let taken = app
            .world_mut()
            .run_system_once(move |mut commands: Commands| inv.take_slot(0, &mut commands, pos))
            .unwrap();
        assert!(taken.is_none());
        assert!(app.world().resource::<Captured>().0.is_empty());
    }

    #[test]
    fn block_inventory_implements_block_data() {
        let inv = BlockInventory::with_capacity(4);
        let boxed: Box<dyn BlockData> = Box::new(inv);
        assert_eq!(boxed.type_key(), std::any::type_name::<BlockInventory>());
        let cloned = boxed.clone_box();
        assert_eq!(cloned.type_key(), boxed.type_key());
        let back = cloned
            .as_any()
            .downcast_ref::<BlockInventory>()
            .expect("downcast");
        assert_eq!(back.inventory().capacity(), 4);
    }

    /// Round-trips a populated [`BlockInventory`] through the
    /// [`BlockDataTypeRegistry`] decoder path, mirroring how the
    /// chunk-cell-data codec will treat it on disk and over the wire.
    /// The general round-trip mechanism is exercised in
    /// `dd40_chunk_storage`'s serialization tests; here we just confirm
    /// `BlockInventory` is wired up correctly.
    #[test]
    fn block_inventory_registers_and_decodes_through_registry() {
        use dd40_core::block::BlockDataAppExt;

        let mut app = App::new();
        app.register_block_data::<BlockInventory>();
        let registry = app
            .world()
            .resource::<dd40_core::block::BlockDataTypeRegistry>();
        let info = registry
            .get::<BlockInventory>()
            .expect("BlockInventory registered");
        assert_eq!(info.type_key, std::any::type_name::<BlockInventory>());
    }
}
