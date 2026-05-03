//! Root plugin for the `dd40_item_core` crate.
//!
//! [`ItemCorePlugin`] is the single entry point.  Add it once to register all
//! item vocabulary.  Implementation crates that depend on this one should
//! call `ensure_plugins!(app, ItemCorePlugin)` from their own `Plugin::build`
//! so consumers do not need to add it manually.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

/// Registers the item-system vocabulary.
///
/// ## What this plugin sets up
///
/// Currently nothing — types and messages will be added in subsequent
/// commits.  The empty `build` body is intentional: this commit only proves
/// the crate scaffold compiles and integrates with the workspace.
#[derive(Default)]
pub struct ItemCorePlugin;

impl Plugin for ItemCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
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
}
