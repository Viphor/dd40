//! Root plugin for the `dd40_texture_pack` crate.
//!
//! [`TexturePackPlugin`] is the single entry point.  It runs a
//! [`Startup`] system that:
//!
//! 1. Walks every directory in
//!    [`TexturePackConfig::search_paths`](crate::TexturePackConfig)
//!    via [`crate::discover`].
//! 2. Decodes each PNG, parses its `.mcmeta`, and classifies its
//!    render layer via [`crate::decode`].  Per-key errors are logged
//!    at `warn!` but do not abort the load.
//! 3. Computes a uniform-grid atlas layout via [`crate::pack`].
//! 4. Builds the pixel buffer + uploads a 2D-array
//!    [`bevy::image::Image`] via [`crate::build`].
//! 5. Installs the resulting [`crate::StaticBlockAtlasSource`] on the
//!    [`BlockAtlas`](dd40_texture_core::BlockAtlas) resource.
//!
//! The startup system lives in the
//! [`AtlasReady`](dd40_texture_core::AtlasReady) system set so
//! consumers can `.after(AtlasReady)` to wait for it.

use std::sync::Arc;

use bevy::asset::Assets;
use bevy::image::Image;
use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_texture_core::{AtlasReady, BlockAtlas, TextureCorePlugin};

use crate::build::install;
use crate::config::TexturePackConfig;
use crate::decode::decode_all;
use crate::discover::discover;
use crate::pack::compute_layout;

const OVERRIDE_PATH_KEY: &str = "DD40_TEXTURE_PACK__OVERRIDE_PATH";

/// Loads Minecraft-style texture packs into a [`BlockAtlas`].
///
/// See the module docs for the load pipeline.  Plugin behaviour:
///
/// - Auto-adds [`CorePlugin`] and [`TextureCorePlugin`] via
///   [`ensure_plugins!`].
/// - Inserts a default [`TexturePackConfig`] if the binary did not
///   provide one (in which case the load pipeline runs over an empty
///   search-path list — discovery returns nothing, no atlas is
///   installed, [`BlockAtlas`] stays empty, and consumers fall back
///   to colour rendering).
#[derive(Default)]
pub struct TexturePackPlugin;

impl Plugin for TexturePackPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, TextureCorePlugin);
        app.init_resource::<TexturePackConfig>()
            .add_systems(Startup, build_and_install_atlas.in_set(AtlasReady));
    }
}

fn build_and_install_atlas(
    config: Res<TexturePackConfig>,
    mut images: ResMut<Assets<Image>>,
    mut atlas: ResMut<BlockAtlas>,
) {
    let search_paths = if let Ok(override_path) = std::env::var(OVERRIDE_PATH_KEY) {
        debug!(
            "dd40_texture_pack: using override search path from env var `{OVERRIDE_PATH_KEY}`: \
             {override_path}"
        );
        config
            .search_paths
            .clone()
            .into_iter()
            .chain(std::iter::once(override_path.into()))
            .collect()
    } else {
        config.search_paths.clone()
    };
    let discovered = discover(&search_paths);
    if discovered.is_empty() {
        debug!(
            "dd40_texture_pack: no textures discovered in {} search path(s); leaving \
             BlockAtlas empty (consumers will use colour fallback)",
            search_paths.len()
        );
        return;
    }

    let (decoded, errors) = decode_all(&discovered, &config);
    for (key, err) in &errors {
        warn!("dd40_texture_pack: skipping `{key}`: {err}");
    }
    if decoded.is_empty() {
        warn!(
            "dd40_texture_pack: discovered {} texture(s) but all failed to decode",
            discovered.len()
        );
        return;
    }

    let (_id, layout) = compute_layout(&decoded);
    info!(
        "dd40_texture_pack: built atlas {}x{} (tile {}px, {} layer(s), {} texture(s))",
        layout.width(),
        layout.height(),
        layout.tile_size,
        layout.layers,
        layout.placements.len(),
    );
    let (source, _id) = install(layout, &decoded, &mut images);
    let source: Arc<dyn dd40_texture_core::BlockAtlasSource> = source;
    atlas.set_source(source);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::image::ImagePlugin;
    use bevy::state::app::StatesPlugin;
    use dd40_texture_core::{TextureCorePlugin, TextureRef};
    use image::{ImageBuffer, Rgba};
    use std::path::Path;
    use tempfile::TempDir;

    fn min_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            bevy::diagnostic::FrameCountPlugin,
            AssetPlugin::default(),
            ImagePlugin::default(),
            StatesPlugin,
        ));
        app
    }

    fn write_solid(path: &Path, fill: [u8; 4]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(4, 4, Rgba(fill));
        buf.save(path).unwrap();
    }

    #[test]
    fn auto_adds_dependencies() {
        let mut app = min_app();
        app.add_plugins(TexturePackPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<TextureCorePlugin>());
    }

    #[test]
    fn inserts_default_config_if_absent() {
        let mut app = min_app();
        app.add_plugins(TexturePackPlugin);
        let cfg = app.world().resource::<TexturePackConfig>();
        assert!(cfg.search_paths.is_empty());
    }

    #[test]
    fn respects_caller_provided_config() {
        let mut app = min_app();
        app.insert_resource(TexturePackConfig::with_search_path("override-me"))
            .add_plugins(TexturePackPlugin);
        let cfg = app.world().resource::<TexturePackConfig>();
        assert_eq!(cfg.search_paths.len(), 1);
        assert_eq!(cfg.search_paths[0].file_name().unwrap(), "override-me");
    }

    #[test]
    fn startup_installs_atlas_for_a_real_pack() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_solid(
            &root.join("assets/minecraft/textures/block/stone.png"),
            [120, 120, 120, 255],
        );

        let mut app = min_app();
        app.insert_resource(TexturePackConfig::with_search_path(root))
            .add_plugins(TexturePackPlugin);
        app.update();

        let atlas = app.world().resource::<BlockAtlas>();
        assert!(
            atlas.is_ready(),
            "BlockAtlas should have a source installed"
        );
        let resolved = atlas
            .resolve(&TextureRef::named("minecraft:block/stone"))
            .expect("stone should resolve");
        assert_eq!(
            resolved.render_layer,
            dd40_texture_core::RenderLayer::Opaque
        );
    }

    #[test]
    fn empty_search_path_leaves_atlas_empty() {
        let mut app = min_app();
        app.add_plugins(TexturePackPlugin);
        app.update();
        let atlas = app.world().resource::<BlockAtlas>();
        assert!(!atlas.is_ready());
    }
}
