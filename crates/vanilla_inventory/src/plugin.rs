//! Root plugin for the `dd40_vanilla_inventory` crate.
//!
//! [`VanillaInventoryPlugin`] is the single entry point.  In v1 the
//! inventory is local-only — add this plugin from `dd40_client` only.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_input_core::plugin::InputCorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

/// Plugin that wires the vanilla inventory rules into the client app.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`], [`InventoryCorePlugin`],
///   [`ItemCorePlugin`], and [`InputCorePlugin`] via
///   [`ensure_plugins!`][dd40_core::ensure_plugins].
/// - Ensures every [`Player`][dd40_character_core::components::Player]
///   has a `SelectedHotbarSlot` and `ActiveItem` component.
/// - Hotbar selection: number-key presses
///   (`HotbarSelect`) and mouse-wheel scroll shift the selected slot.
/// - `RequestActiveItem` bridge: external requests pick a matching
///   hotbar slot when possible.
/// - `SlotInteraction` apply: drains click/drop messages, mutates the
///   targeted inventory and the global `HeldStack`, and emits
///   `DropItems` for drop-outside intent.
/// - Observer that keeps `ActiveItem` in sync with the selected slot
///   when the inventory mutates.
#[derive(Default)]
pub struct VanillaInventoryPlugin;

impl Plugin for VanillaInventoryPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            InventoryCorePlugin,
            ItemCorePlugin,
            InputCorePlugin
        );
        app.add_systems(
            Update,
            (
                crate::selection::ensure_selected_slot,
                crate::selection::ensure_active_item,
                crate::selection::apply_hotbar_keys,
                crate::selection::apply_hotbar_wheel,
                crate::selection::apply_active_item_requests,
                crate::selection::sync_active_item_on_slot_change,
                crate::apply::apply_slot_interactions,
            ),
        );
        app.add_observer(crate::selection::sync_active_item_on_inventory_change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_adds_foundation_plugins() {
        let mut app = App::new();
        app.add_plugins(VanillaInventoryPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<InventoryCorePlugin>());
        assert!(app.is_plugin_added::<ItemCorePlugin>());
        assert!(app.is_plugin_added::<InputCorePlugin>());
    }
}
