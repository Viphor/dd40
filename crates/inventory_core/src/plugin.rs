//! Root plugin for the `dd40_inventory_core` crate.
//!
//! [`InventoryCorePlugin`] is the single entry point.  Add it once to
//! register the [`InventoryComponent`] for reflection and the
//! [`BlockInventory`] type with the chunk's cell-data registry.
//! Implementation crates that depend on this one should call
//! `ensure_plugins!(app, InventoryCorePlugin)` from their own
//! `Plugin::build` so consumers do not need to add it manually.

use bevy::prelude::*;
use dd40_core::block::BlockDataAppExt;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

use crate::block::BlockInventory;
use crate::component::InventoryComponent;
use crate::drop::DropItems;
use crate::held_stack::HeldStackComponent;
use crate::set_active_slot::SetActiveSlot;
use crate::slot_interaction::SlotInteraction;

/// Registers the inventory-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`] and [`ItemCorePlugin`] via
///   [`ensure_plugins!`][dd40_core::ensure_plugins].
/// - Registers [`InventoryComponent`] and [`HeldStackComponent`] for
///   reflection.
/// - Registers [`BlockInventory`] with the block-data type registry so
///   chunk cell data can carry it over the wire and on disk.
/// - Registers the [`DropItems`], [`SlotInteraction`], and
///   [`SetActiveSlot`] messages.
/// - When the `player_storage` feature is enabled, registers
///   `InventoryContributor` into [`PlayerStateRegistry`] so inventory is
///   persisted across sessions.
///
/// [`InventoryChanged`][crate::component::InventoryChanged] and
/// [`BlockInventoryChanged`][crate::block::BlockInventoryChanged] are
/// `Event`s, not `Message`s, so they do not need explicit registration
/// — observers register themselves with `app.add_observer(...)`.
#[derive(Default)]
pub struct InventoryCorePlugin;

impl Plugin for InventoryCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, ItemCorePlugin);
        app.register_type::<InventoryComponent>();
        app.register_type::<HeldStackComponent>();
        app.register_block_data::<BlockInventory>();
        app.add_message::<DropItems>();
        app.add_message::<SlotInteraction>();
        app.add_message::<SetActiveSlot>();

        #[cfg(feature = "player_storage")]
        {
            use dd40_core::ensure_plugins;
            use dd40_player_storage::{PlayerStoragePlugin, PlayerStateRegistry};

            ensure_plugins!(app, PlayerStoragePlugin);
            app.world_mut()
                .resource_mut::<PlayerStateRegistry>()
                .register(crate::player_storage::InventoryContributor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_adds_core_plugin() {
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        assert!(
            app.is_plugin_added::<CorePlugin>(),
            "CorePlugin must be auto-added by InventoryCorePlugin"
        );
    }

    #[test]
    fn auto_adds_item_core_plugin() {
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        assert!(
            app.is_plugin_added::<ItemCorePlugin>(),
            "ItemCorePlugin must be auto-added by InventoryCorePlugin"
        );
    }

    #[test]
    fn registers_block_inventory_with_block_data_registry() {
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        let registry = app
            .world()
            .resource::<dd40_core::block::BlockDataTypeRegistry>();
        assert!(
            registry.get::<BlockInventory>().is_some(),
            "BlockInventory must be registered with the BlockDataTypeRegistry"
        );
    }

    #[test]
    fn registers_drop_items_message() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        assert!(
            app.world().get_resource::<Messages<DropItems>>().is_some(),
            "DropItems must be registered as a Bevy message"
        );
    }

    #[test]
    fn registers_slot_interaction_message() {
        use bevy::ecs::message::Messages;
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        assert!(
            app.world()
                .get_resource::<Messages<SlotInteraction>>()
                .is_some(),
            "SlotInteraction must be registered as a Bevy message"
        );
    }

    #[test]
    fn registers_held_stack_component_for_reflection() {
        let mut app = App::new();
        app.add_plugins(InventoryCorePlugin);
        let registry = app
            .world()
            .resource::<bevy::ecs::reflect::AppTypeRegistry>()
            .read();
        assert!(
            registry
                .get(std::any::TypeId::of::<HeldStackComponent>())
                .is_some(),
            "HeldStackComponent must be registered for reflection"
        );
    }
}
