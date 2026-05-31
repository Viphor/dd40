//! Root plugin for the `dd40_texture_core` crate.
//!
//! [`TextureCorePlugin`] is the single entry point.  Add it once to
//! register [`BlockTextures`] and [`RenderLayer`] with the
//! [`BlockDataTypeRegistry`] so they round-trip through cell-data
//! serialisation, and to insert the default empty [`BlockAtlas`]
//! resource so consumers can always query it safely.
//!
//! [`BlockDataTypeRegistry`]: dd40_core::block::BlockDataTypeRegistry

use bevy::prelude::*;
use dd40_core::block::BlockDataAppExt;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

use crate::atlas::BlockAtlas;
use crate::block_textures::BlockTextures;
use crate::render_layer::RenderLayer;

/// Registers the texture-system vocabulary.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`] via [`ensure_plugins!`].
/// - Registers [`BlockTextures`] and [`RenderLayer`] with the
///   [`BlockDataTypeRegistry`] so they can be attached as default
///   block data on a
///   [`BlockDefinition`][dd40_core::block::BlockDefinition].
/// - Inserts a default [`BlockAtlas`] resource (with no installed
///   source) so consumers can call `resolve` safely before any pack
///   has loaded.
///
/// [`BlockDataTypeRegistry`]: dd40_core::block::BlockDataTypeRegistry
#[derive(Default)]
pub struct TextureCorePlugin;

impl Plugin for TextureCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
        app.register_block_data::<BlockTextures>()
            .register_block_data::<RenderLayer>()
            .init_resource::<BlockAtlas>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockDataTypeRegistry;

    #[test]
    fn auto_adds_core_plugin() {
        let mut app = App::new();
        app.add_plugins(TextureCorePlugin);
        assert!(
            app.is_plugin_added::<CorePlugin>(),
            "CorePlugin must be auto-added by TextureCorePlugin"
        );
    }

    #[test]
    fn registers_block_textures_and_render_layer() {
        let mut app = App::new();
        app.add_plugins(TextureCorePlugin);
        let registry = app.world().resource::<BlockDataTypeRegistry>();
        assert!(
            registry.get::<BlockTextures>().is_some(),
            "BlockTextures must be registered with the BlockDataTypeRegistry"
        );
        assert!(
            registry.get::<RenderLayer>().is_some(),
            "RenderLayer must be registered with the BlockDataTypeRegistry"
        );
    }

    #[test]
    fn inserts_default_block_atlas_resource() {
        let mut app = App::new();
        app.add_plugins(TextureCorePlugin);
        let atlas = app.world().resource::<BlockAtlas>();
        assert!(
            !atlas.is_ready(),
            "Default BlockAtlas should be inserted with no source installed"
        );
    }
}
