//! [`RegisterConfig`] — an [`App`] extension for eager config registration.

use bevy::prelude::*;

use crate::{ConfigSection, RawConfig};

/// Extension trait that adds [`register_config`][RegisterConfig::register_config]
/// to Bevy's [`App`].
pub trait RegisterConfig {
    /// Load a [`ConfigSection`] from [`RawConfig`] and insert it as a typed
    /// [`Resource`] immediately — during `Plugin::build`, before any system
    /// runs.
    ///
    /// This avoids the ordering hazard of loading configs inside a `Startup`
    /// system: because the resource is inserted eagerly, every `Startup`
    /// system (regardless of ordering) can rely on it already being present.
    ///
    /// # Panics
    ///
    /// Does not panic. If [`RawConfig`] is not yet present (i.e. [`ConfigPlugin`]
    /// has not been added), the section falls back to `T::default()`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bevy::prelude::*;
    /// use dd40_config::{ConfigPlugin, ConfigSection, RegisterConfig};
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Debug, Clone, Deserialize, Serialize, Default, Resource)]
    /// #[serde(default)]
    /// pub struct MyConfig { pub value: u32 }
    ///
    /// impl ConfigSection for MyConfig {
    ///     const SECTION: &'static str = "my_plugin";
    /// }
    ///
    /// pub struct MyPlugin;
    ///
    /// impl Plugin for MyPlugin {
    ///     fn build(&self, app: &mut App) {
    ///         // ConfigPlugin must be added first.
    ///         app.register_config::<MyConfig>();
    ///         // MyConfig is now available as Res<MyConfig> in every system.
    ///     }
    /// }
    /// ```
    ///
    /// [`ConfigPlugin`]: crate::ConfigPlugin
    fn register_config<T>(&mut self) -> &mut Self
    where
        T: ConfigSection + Resource;
}

impl RegisterConfig for App {
    fn register_config<T>(&mut self) -> &mut Self
    where
        T: ConfigSection + Resource,
    {
        let cfg = self
            .world()
            .get_resource::<RawConfig>()
            .map(|raw| raw.section::<T>())
            .unwrap_or_default();
        self.insert_resource(cfg);
        self
    }
}
