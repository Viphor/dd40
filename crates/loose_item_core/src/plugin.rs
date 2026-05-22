//! Root plugin for `dd40_loose_item_core`.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

use crate::{
    components::{DespawnTimer, LooseItem, PickupCooldown},
    resources::LooseItemConfig,
    system_sets::LooseItemSet,
};

/// Foundation plugin: registers loose-item types, the
/// [`LooseItemConfig`] resource (with defaults), and the
/// [`LooseItemSet`] ordering.
///
/// This is a **Tier 0 foundation plugin**: it adds no game systems.
/// Implementation crates (`dd40_loose_items`,
/// `dd40_integration_loose_item_pickup`, …) call
/// `ensure_plugins!(app, LooseItemCorePlugin)` and then attach their
/// systems to [`LooseItemSet`].
#[derive(Default)]
pub struct LooseItemCorePlugin;

impl Plugin for LooseItemCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);

        app.register_type::<LooseItem>()
            .register_type::<DespawnTimer>()
            .register_type::<PickupCooldown>()
            .register_type::<LooseItemConfig>()
            .init_resource::<LooseItemConfig>()
            .configure_sets(
                Update,
                (
                    LooseItemSet::Spawn,
                    LooseItemSet::Attract,
                    LooseItemSet::Resolve,
                    LooseItemSet::Lifecycle,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_initialises_config_with_defaults() {
        let mut app = App::new();
        app.add_plugins(LooseItemCorePlugin);
        let cfg = app.world().resource::<LooseItemConfig>();
        assert_eq!(cfg.default_lifetime.as_secs(), 5 * 60);
        assert_eq!(cfg.default_pickup_cooldown.as_millis(), 500);
        assert!((cfg.attraction_radius - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn plugin_is_idempotent_via_ensure_plugins() {
        // ensure_plugins! protects nested calls — simulate a downstream
        // crate's plugin that depends on LooseItemCorePlugin.
        #[derive(Default)]
        struct DownstreamPlugin;
        impl Plugin for DownstreamPlugin {
            fn build(&self, app: &mut App) {
                ensure_plugins!(app, LooseItemCorePlugin);
            }
        }

        let mut app = App::new();
        // User adds the core plugin explicitly.
        app.add_plugins(LooseItemCorePlugin);
        // Downstream plugin tries to also ensure it.
        app.add_plugins(DownstreamPlugin);
        assert!(app.world().contains_resource::<LooseItemConfig>());
    }
}
