//! Root plugin for the `dd40_vanilla_inventory` crate.
//!
//! The crate ships **three** plugins so the client/server split is
//! explicit:
//!
//! - [`VanillaInventoryActiveItemPlugin`] — attaches
//!   [`ActiveItem`][dd40_item_core::active_item::ActiveItem] (with an
//!   [`InventorySlotProvider`][crate::active_item::InventorySlotProvider])
//!   to every entity that gains an `InventoryComponent`, and keeps
//!   its cache in sync.  Add this on **both** client and server.
//! - [`VanillaInventoryPlugin`] — hotbar input (number keys, mouse
//!   wheel), [`RequestActiveItem`][dd40_item_core::messages::RequestActiveItem]
//!   bridge.  Pure intent translation; emits
//!   [`SetActiveSlot`][dd40_inventory_core::set_active_slot::SetActiveSlot]
//!   messages.  Add this on the **client only** in a networked build.
//! - [`VanillaInventoryRulesPlugin`] — the authoritative apply system
//!   that drains `SlotInteraction` and `SetActiveSlot` and mutates
//!   `InventoryComponent`.  Add this on the **server only** in a
//!   networked build.  Single-player binaries may add it on the
//!   client.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_input_core::plugin::InputCorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;

pub use crate::active_item::VanillaInventoryActiveItemPlugin;

/// Plugin that wires the vanilla inventory **input** layer.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`], [`InventoryCorePlugin`],
///   [`ItemCorePlugin`], and [`InputCorePlugin`] via
///   [`ensure_plugins!`][dd40_core::ensure_plugins].
/// - Hotbar selection: number-key (`HotbarSelect`) and mouse-wheel
///   input emits [`SetActiveSlot`][dd40_inventory_core::set_active_slot::SetActiveSlot]
///   messages targeting the local [`Player`][dd40_character_core::components::Player].
/// - [`RequestActiveItem`][dd40_item_core::messages::RequestActiveItem]
///   bridge: external requests pick a matching slot in the recipient's
///   inventory and emit a `SetActiveSlot`.
///
/// Does **not** mutate any inventory state itself.  See
/// [`VanillaInventoryRulesPlugin`] for the apply system that
/// authoritatively consumes the emitted `SetActiveSlot` messages.
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
                crate::selection::apply_hotbar_keys,
                crate::selection::apply_hotbar_wheel,
                crate::selection::apply_active_item_requests,
            ),
        );
    }
}

/// Plugin that adds the authoritative apply systems for
/// `SlotInteraction` and `SetActiveSlot`.
///
/// Add this on the **server** in a networked build; the server's
/// `ServerInventoryNetworkPlugin` translates incoming wire messages
/// onto the local message buses these systems consume.
/// Single-player builds may also add it on the client.
#[derive(Default)]
pub struct VanillaInventoryRulesPlugin;

impl Plugin for VanillaInventoryRulesPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, InventoryCorePlugin, ItemCorePlugin);
        app.add_systems(
            Update,
            (
                crate::apply::apply_slot_interactions,
                crate::selection::apply_set_active_slot,
            ),
        );
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

    #[test]
    fn active_item_plugin_auto_adds_foundation_plugins() {
        let mut app = App::new();
        app.add_plugins(VanillaInventoryActiveItemPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<InventoryCorePlugin>());
        assert!(app.is_plugin_added::<ItemCorePlugin>());
    }
}
