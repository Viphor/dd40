//! Root plugin for `dd40_loose_items`.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::persistence::EntityPersisterRegistry;
use dd40_core::plugin::CorePlugin;
use dd40_inventory_core::plugin::InventoryCorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_loose_item_core::plugin::LooseItemCorePlugin;
use dd40_loose_item_core::system_sets::LooseItemSet;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::merge::{MergeAccumulator, accumulate_and_merge_loose_items};
use crate::persister::{LOOSE_ITEM_KIND, LooseItemPersister};
use crate::spawn::{spawn_loose_items, tick_lifetimes};

/// Server-side plugin: spawns loose items from
/// `DropItems` messages and ticks
/// their despawn + cooldown timers.
///
/// Add this to the **server** binary only. Clients receive loose
/// items via replication (slice 11) and never spawn them locally.
#[derive(Default)]
pub struct LooseItemsPlugin;

impl Plugin for LooseItemsPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(
            app,
            CorePlugin,
            PhysicsCorePlugin,
            ItemCorePlugin,
            InventoryCorePlugin,
            LooseItemCorePlugin,
        );

        app.init_resource::<MergeAccumulator>()
            .add_systems(Update, spawn_loose_items.in_set(LooseItemSet::Spawn))
            .add_systems(
                Update,
                accumulate_and_merge_loose_items.in_set(LooseItemSet::Resolve),
            )
            .add_systems(Update, tick_lifetimes.in_set(LooseItemSet::Lifecycle));

        // Register the persister exactly once even if the plugin is
        // added through multiple ensure_plugins! paths.
        let mut registry = app.world_mut().resource_mut::<EntityPersisterRegistry>();
        if registry.by_kind(LOOSE_ITEM_KIND).is_none() {
            registry.register(LooseItemPersister);
        }
    }
}
