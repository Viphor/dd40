//! Root plugin for the `dd40_texture_pack` crate.
//!
//! [`TexturePackPlugin`] is the single entry point.  In this commit it
//! only wires up its foundation dependencies and ensures a default
//! [`TexturePackConfig`] is present; PNG decoding, atlas building,
//! and [`BlockAtlas`](dd40_texture_core::BlockAtlas) population land in
//! follow-up commits.

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_texture_core::TextureCorePlugin;

use crate::config::TexturePackConfig;

/// Loads Minecraft-style texture packs into a [`BlockAtlas`].
///
/// [`BlockAtlas`]: dd40_texture_core::BlockAtlas
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`] and [`TextureCorePlugin`] via
///   [`ensure_plugins!`].
/// - Inserts a default [`TexturePackConfig`] if the binary did not
///   provide one.  Add your own
///   [`insert_resource(TexturePackConfig { ... })`]
///   before this plugin is added to override.
///
/// Decode + upload systems are added in follow-up commits.
#[derive(Default)]
pub struct TexturePackPlugin;

impl Plugin for TexturePackPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, TextureCorePlugin);
        app.init_resource::<TexturePackConfig>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_texture_core::TextureCorePlugin;

    #[test]
    fn auto_adds_dependencies() {
        let mut app = App::new();
        app.add_plugins(TexturePackPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<TextureCorePlugin>());
    }

    #[test]
    fn inserts_default_config_if_absent() {
        let mut app = App::new();
        app.add_plugins(TexturePackPlugin);
        let cfg = app.world().resource::<TexturePackConfig>();
        assert!(cfg.search_paths.is_empty());
    }

    #[test]
    fn respects_caller_provided_config() {
        let mut app = App::new();
        app.insert_resource(TexturePackConfig::with_search_path("override-me"))
            .add_plugins(TexturePackPlugin);
        let cfg = app.world().resource::<TexturePackConfig>();
        assert_eq!(cfg.search_paths.len(), 1);
        assert_eq!(cfg.search_paths[0].file_name().unwrap(), "override-me");
    }
}
