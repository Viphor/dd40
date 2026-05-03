//! Root plugin for the `dd40_item_core` crate.
//!
//! [`ItemCorePlugin`] is the single entry point.  Add it once to register all
//! item vocabulary.  Implementation crates that depend on this one should
//! call `ensure_plugins!(app, ItemCorePlugin)` from their own `Plugin::build`
//! so consumers do not need to add it manually.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

use crate::registry::ItemRegistry;

/// Registers the item-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Inserts [`ItemRegistry`] as a resource (with the [`ItemId::EMPTY`]
///   sentinel pre-registered) and registers it for reflection.
/// - Configures the [`ItemRegistrySet`] system set.
///
/// [`ItemId::EMPTY`]: crate::registry::ItemId::EMPTY
/// [`ItemRegistrySet`]: crate::registry::ItemRegistrySet
#[derive(Default)]
pub struct ItemCorePlugin;

impl Plugin for ItemCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);

        app.insert_resource(ItemRegistry::new())
            .register_type::<ItemRegistry>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ItemId;

    #[test]
    fn item_core_plugin_auto_adds_core() {
        let mut app = App::new();
        app.add_plugins(ItemCorePlugin);
        assert!(
            app.is_plugin_added::<CorePlugin>(),
            "CorePlugin must be auto-added by ItemCorePlugin"
        );
    }

    #[test]
    fn item_core_plugin_inserts_registry_with_empty_sentinel() {
        let mut app = App::new();
        app.add_plugins(ItemCorePlugin);
        let registry = app
            .world()
            .get_resource::<ItemRegistry>()
            .expect("ItemRegistry inserted");
        assert!(registry.get(ItemId::EMPTY).is_some());
    }
}
