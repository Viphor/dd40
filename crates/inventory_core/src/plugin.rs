//! Root plugin for the `dd40_inventory_core` crate.
//!
//! [`InventoryCorePlugin`] is the single entry point.  Add it once to
//! register the [`Inventory`][crate::inventory::Inventory] component for
//! reflection.  Implementation crates that depend on this one should call
//! `ensure_plugins!(app, InventoryCorePlugin)` from their own
//! `Plugin::build` so consumers do not need to add it manually.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

use crate::inventory::Inventory;

/// Registers the inventory-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`] and [`ItemCorePlugin`] via
///   [`ensure_plugins!`][dd40_core::ensure_plugins].
/// - Registers [`Inventory`] for reflection.
///
/// [`InventoryChanged`][crate::inventory::InventoryChanged] is an `Event`,
/// not a `Message`, so it does not need explicit registration — observers
/// register themselves with `app.add_observer(...)`.
#[derive(Default)]
pub struct InventoryCorePlugin;

impl Plugin for InventoryCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, ItemCorePlugin);
        app.register_type::<Inventory>();
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
}
