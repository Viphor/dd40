//! Root plugin for `dd40_inventory_gui`.
//!
//! [`InventoryGuiPlugin`] installs the hotbar, the grid window, the icon
//! cache, the held-cursor renderer, and the bevy_ui click → slot-interaction
//! translator. Add it to the client `App` exactly once.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_input_core::plugin::InputCorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

use crate::grid;
use crate::held;
use crate::hotbar;
use crate::icons;
use crate::input;

/// SystemSet containing every system this crate adds to [`Update`].
///
/// Downstream binaries can `.before` / `.after` this set to interleave
/// custom HUD systems with the inventory UI.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InventoryGuiSet;

/// Resource flagging whether the inventory grid window is currently open.
///
/// Mirrors the toggle keyed by `ToggleInventory` (default `KeyE`). The
/// grid window's spawn/despawn observers read this; downstream systems
/// can also read it to gate behaviour (e.g. pause world-mouse-look).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InventoryGuiOpen(pub bool);

/// Installs every system this crate provides.
///
/// ## What this plugin sets up
///
/// - [`InventoryGuiOpen`] resource (default closed).
/// - [`icons::ItemIconCache`] resource.
/// - The bottom-centred hotbar (always visible).
/// - The grid window spawn/despawn observer keyed off the
///   `ToggleInventory` BEI action.
/// - The held-stack cursor renderer.
/// - bevy_ui `Interaction` → [`SlotInteraction`](dd40_inventory_core::SlotInteraction)
///   translation.
#[derive(Default)]
pub struct InventoryGuiPlugin;

impl Plugin for InventoryGuiPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            InventoryCorePlugin,
            ItemCorePlugin,
            InputCorePlugin
        );
        app.init_resource::<InventoryGuiOpen>()
            .init_resource::<icons::ItemIconCache>();
        app.add_systems(
            Update,
            (
                hotbar::ensure_hotbar_root,
                hotbar::sync_hotbar_selection,
                grid::toggle_grid,
                grid::ensure_grid_widgets,
                held::sync_held_cursor,
                input::translate_clicks,
                input::translate_drop_outside,
                crate::slot_widget::sync_slot_widgets,
                crate::slot_widget::sync_selection_border,
            )
                .in_set(InventoryGuiSet),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_auto_adds_foundations() {
        let mut app = App::new();
        app.add_plugins(InventoryGuiPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<InventoryCorePlugin>());
        assert!(app.is_plugin_added::<ItemCorePlugin>());
        assert!(app.is_plugin_added::<InputCorePlugin>());
        assert!(app.world().get_resource::<InventoryGuiOpen>().is_some());
    }
}
