//! Root plugin for the `dd40_vanilla_inventory` crate.
//!
//! The crate ships **two** plugins so the client/server split is
//! explicit:
//!
//! - [`VanillaInventoryPlugin`] — selection bookkeeping
//!   (`SelectedHotbarSlot`, `ActiveItem`, hotbar keys, mouse wheel,
//!   `RequestActiveItem` bridge).  Pure UI/derived state; no
//!   inventory mutation.  Add this on **both** client and server.
//! - [`VanillaInventoryRulesPlugin`] — the authoritative apply
//!   system that drains `SlotInteraction` and mutates
//!   `InventoryComponent` + `HeldStackComponent`.  Add this on the
//!   **server only** in a networked build.  Single-player binaries
//!   may add it on the client.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_input_core::plugin::InputCorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

/// Plugin that wires the vanilla inventory **selection** layer.
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
/// - Observer that keeps `ActiveItem` in sync with the selected slot
///   when the inventory mutates.
///
/// Does **not** add the slot-interaction apply system.  See
/// [`VanillaInventoryRulesPlugin`].
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
            ),
        );
        app.add_observer(crate::selection::sync_active_item_on_inventory_change);
    }
}

/// Plugin that adds the authoritative `SlotInteraction` apply system.
///
/// Add this on the **server** in a networked build; the server's
/// `ServerInventoryNetworkPlugin` translates incoming wire messages
/// onto the local `SlotInteraction` bus that this system consumes.
/// Single-player builds may also add it on the client.
#[derive(Default)]
pub struct VanillaInventoryRulesPlugin;

impl Plugin for VanillaInventoryRulesPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            InventoryCorePlugin,
            ItemCorePlugin
        );
        app.add_systems(Update, crate::apply::apply_slot_interactions);
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

    #[test]
    fn rules_plugin_auto_adds_foundation_plugins() {
        let mut app = App::new();
        app.add_plugins(VanillaInventoryRulesPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<InventoryCorePlugin>());
        assert!(app.is_plugin_added::<ItemCorePlugin>());
    }
}
