//! Root plugin for the `dd40_loot_core` crate.
//!
//! [`LootCorePlugin`] is the single entry point.  Add it once to
//! register [`LootTable`] with the chunk's
//! block-data type registry so it can flow through cell-data
//! serialisation just like any other `BlockData`.

use bevy::prelude::*;
use dd40_core::block::BlockDataAppExt;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

use crate::table::LootTable;

/// Registers the loot-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`] via
///   [`ensure_plugins!`][dd40_core::ensure_plugins].
/// - Registers [`LootTable`] with the block-data type registry so it
///   can be attached as default block data on a
///   [`BlockDefinition`][dd40_core::block::BlockDefinition].
#[derive(Default)]
pub struct LootCorePlugin;

impl Plugin for LootCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
        app.register_block_data::<LootTable>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_adds_core_plugin() {
        let mut app = App::new();
        app.add_plugins(LootCorePlugin);
        assert!(
            app.is_plugin_added::<CorePlugin>(),
            "CorePlugin must be auto-added by LootCorePlugin"
        );
    }

    #[test]
    fn registers_loot_table_with_block_data_registry() {
        let mut app = App::new();
        app.add_plugins(LootCorePlugin);
        let registry = app
            .world()
            .resource::<dd40_core::block::BlockDataTypeRegistry>();
        assert!(
            registry.get::<LootTable>().is_some(),
            "LootTable must be registered with the BlockDataTypeRegistry"
        );
    }
}
