//! Root plugin for the loose-item pickup integration.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_loose_item_core::plugin::LooseItemCorePlugin;
use dd40_loose_item_core::system_sets::LooseItemSet;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::attract::attract_loose_items;
use crate::pickup::pickup_loose_items;

/// Server-only plugin: subscribes to `BodyBodyContact` and grants
/// loose items to characters whose inventory has room.
///
/// Add it to the **server** binary only — clients never run the
/// pickup logic; they observe the resulting entity despawn and
/// inventory updates over replication.
#[derive(Default)]
pub struct LooseItemPickupPlugin;

impl Plugin for LooseItemPickupPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            PhysicsCorePlugin,
            ItemCorePlugin,
            InventoryCorePlugin,
            LooseItemCorePlugin,
        );

        app.add_systems(Update, attract_loose_items.in_set(LooseItemSet::Attract))
            .add_systems(Update, pickup_loose_items.in_set(LooseItemSet::Resolve));
    }
}
