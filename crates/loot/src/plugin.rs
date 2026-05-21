//! [`LootPlugin`] — server-only entry point.

use bevy::prelude::*;

use dd40_core::chunk::authority::ChunkAuthoritySet;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_loot_core::plugin::LootCorePlugin;
use dd40_rng::RngPlugin;

use crate::system::{PendingDropSnapshots, emit_loot_drops, snapshot_remove_targets};

/// System sets owned by the loot pipeline.
///
/// `EmitDrops` runs in [`PostUpdate`] **after**
/// [`ChunkAuthoritySet::Commit`] so it can read the `ChunkChanged`
/// messages produced by the commit pass in the same frame.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum LootSet {
    /// `emit_loot_drops` runs here, after the chunk-authority commit.
    EmitDrops,
}

/// Server-only loot plugin.
///
/// Adding this plugin to the **server** binary is what turns
/// destroyed blocks into
/// [`DropItems`][dd40_inventory_core::drop::DropItems] messages. The
/// client never adds it; clients learn about drops by receiving the
/// replicated item entities (forthcoming).
///
/// Auto-adds [`CorePlugin`], [`ItemCorePlugin`],
/// [`InventoryCorePlugin`], [`LootCorePlugin`], and
/// [`RngPlugin`] so callers do not have to.
#[derive(Default)]
pub struct LootPlugin;

impl Plugin for LootPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            ItemCorePlugin,
            InventoryCorePlugin,
            LootCorePlugin,
            RngPlugin,
        );
        app.init_resource::<PendingDropSnapshots>();
        app.configure_sets(
            PostUpdate,
            LootSet::EmitDrops.after(ChunkAuthoritySet::Commit),
        );
        app.add_systems(
            PostUpdate,
            (
                snapshot_remove_targets.in_set(ChunkAuthoritySet::Validate),
                emit_loot_drops.in_set(LootSet::EmitDrops),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_adds_dependencies() {
        let mut app = App::new();
        app.add_plugins(LootPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<ItemCorePlugin>());
        assert!(app.is_plugin_added::<InventoryCorePlugin>());
        assert!(app.is_plugin_added::<LootCorePlugin>());
        assert!(app.is_plugin_added::<RngPlugin>());
    }

    #[test]
    fn inserts_pending_drop_snapshots_resource() {
        let mut app = App::new();
        app.add_plugins(LootPlugin);
        assert!(app.world().get_resource::<PendingDropSnapshots>().is_some());
    }
}
