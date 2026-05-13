//! Root plugin for the `dd40_item_core` crate.
//!
//! [`ItemCorePlugin`] is the single entry point.  Add it once to register all
//! item vocabulary.  Implementation crates that depend on this one should
//! call `ensure_plugins!(app, ItemCorePlugin)` from their own `Plugin::build`
//! so consumers do not need to add it manually.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

use crate::active_item::ActiveItem;
use crate::messages::RequestActiveItem;
use crate::registry::ItemRegistry;

/// Registers the item-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Inserts an empty [`ItemRegistry`] as a resource and registers it for
///   reflection.
/// - Registers [`ActiveItem`] for reflection.
/// - Registers the [`RequestActiveItem`] message.
/// - Configures the [`ItemRegistrySet`] system set.
///
/// [`ActiveItemChanged`][crate::messages::ActiveItemChanged] is an
/// `EntityEvent`, not a `Message`, so it does not need explicit
/// registration — observers register themselves with
/// `app.add_observer(...)`.
///
/// [`ItemRegistrySet`]: crate::registry::ItemRegistrySet
#[derive(Default)]
pub struct ItemCorePlugin;

impl Plugin for ItemCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);

        app.insert_resource(ItemRegistry::new())
            .register_type::<ItemRegistry>()
            .register_type::<ActiveItem>()
            .add_message::<RequestActiveItem>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn item_core_plugin_inserts_empty_registry() {
        let mut app = App::new();
        app.add_plugins(ItemCorePlugin);
        let registry = app
            .world()
            .get_resource::<ItemRegistry>()
            .expect("ItemRegistry inserted");
        assert_eq!(registry.iter().count(), 0);
    }
}
