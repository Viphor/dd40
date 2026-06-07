//! Root plugin for the `dd40_texture_pack` crate.
//!
//! [`TexturePackPlugin`] is the single entry point.  It runs a
//! [`Startup`] system that:
//!
//! 1. Collects search paths from the programmatic [`TexturePackConfig`]
//!    resource *and* any additional paths listed in the `[texture_pack]`
//!    section of `config.toml` (via [`dd40_config::RawConfig`], if present).
//! 2. Walks every directory via [`crate::discover`].
//! 3. Decodes each PNG, parses its `.mcmeta`, and classifies its
//!    render layer via [`crate::decode`].  Per-key errors are logged
//!    at `warn!` but do not abort the load.
//! 4. Computes a uniform-grid atlas layout via [`crate::pack`].
//! 5. Builds the pixel buffer + uploads a 2D-array
//!    [`bevy::image::Image`] via [`crate::build`].
//! 6. Installs the resulting [`crate::StaticBlockAtlasSource`] on the
//!    [`BlockAtlas`](dd40_texture_core::BlockAtlas) resource.
//!
//! The startup system lives in the
//! [`AtlasReady`](dd40_texture_core::AtlasReady) system set so
//! consumers can `.after(AtlasReady)` to wait for it.

use std::sync::Arc;

use bevy::asset::Assets;
use bevy::image::Image;
use bevy::prelude::*;
use dd40_config::{ConfigSection, RawConfig};
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_texture_core::{AtlasReady, BlockAtlas, TextureCorePlugin};
use serde::{Deserialize, Serialize};

use crate::build::install;
use crate::config::TexturePackConfig;
use crate::decode::decode_all;
use crate::discover::discover;
use crate::pack::compute_layout;

/// Config section for additional texture-pack search paths loaded from
/// `config.toml`.
///
/// Paths listed here are appended *after* the programmatic
/// [`TexturePackConfig::search_paths`], so user-supplied packs take
/// precedence over bundled ones.
///
/// ```toml
/// [texture_pack]
/// search_paths = ["/home/user/.minecraft/resourcepacks/fancy"]
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TexturePackTomlConfig {
    /// Extra pack-root directories to search for block textures.
    pub search_paths: Vec<std::path::PathBuf>,
}

impl ConfigSection for TexturePackTomlConfig {
    const SECTION: &'static str = "texture_pack";
}

/// Loads Minecraft-style texture packs into a [`BlockAtlas`].
///
/// See the module docs for the load pipeline.  Plugin behaviour:
///
/// - Auto-adds [`CorePlugin`] and [`TextureCorePlugin`] via
///   [`ensure_plugins!`].
/// - Inserts a default [`TexturePackConfig`] if the binary did not
///   provide one (in which case only config-file paths are searched;
///   if those are also empty, no atlas is installed and [`BlockAtlas`]
///   stays empty, causing consumers to fall back to colour rendering).
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
    raw_config: Option<Res<RawConfig>>,
    mut images: ResMut<Assets<Image>>,
    mut atlas: ResMut<BlockAtlas>,
) {
    // Programmatic paths (set by the binary / tests) come first.
    // Config-file paths are appended so users can override bundled textures.
    let search_paths: Vec<_> = config
        .search_paths
        .iter()
        .cloned()
        .chain(
            raw_config
                .as_ref()
                .map(|r| r.section::<TexturePackTomlConfig>().search_paths)
                .unwrap_or_default(),
        )
        .collect();

    let discovered = discover(&search_paths);
    if discovered.is_empty() {
        if !search_paths.is_empty() {
            warn!(
                paths = ?search_paths,
                "dd40_texture_pack: no textures found in any search path"
            );
        }
        return;
    }
    let (decoded, errors) = decode_all(&discovered, &config);
    for (key, err) in &errors {
        warn!("dd40_texture_pack: skipping `{key}`: {err}");
    }
    if decoded.is_empty() {
        warn!(
            "dd40_texture_pack: discovered {} file(s) but none decoded successfully",
            discovered.len()
        );
        return;
    }
    let (_id, layout) = compute_layout(&decoded);
    info!(
        "dd40_texture_pack: built atlas {}x{} ({} tile size, {} layer(s), {} texture(s))",
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
    use bevy::asset::AssetPlugin;
    use bevy::image::ImagePlugin;
    use bevy::state::app::StatesPlugin;
    use dd40_texture_core::{TextureCorePlugin, TextureRef};
    use image::{ImageBuffer, Rgba};
    use std::path::Path;
    use tempfile::TempDir;

    use super::*;

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

    #[test]
    fn config_file_search_paths_are_appended() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_solid(
            &root.join("assets/minecraft/textures/block/stone.png"),
            [120, 120, 120, 255],
        );

        let mut raw = toml::Table::new();
        let mut section = toml::Table::new();
        let paths_array = toml::Value::Array(vec![toml::Value::String(
            root.to_string_lossy().to_string(),
        )]);
        section.insert("search_paths".to_string(), paths_array);
        raw.insert("texture_pack".to_string(), toml::Value::Table(section));

        let mut app = min_app();
        // No programmatic search paths — paths come entirely from RawConfig.
        app.insert_resource(dd40_config::RawConfig(raw))
            .add_plugins(TexturePackPlugin);
        app.update();

        let atlas = app.world().resource::<BlockAtlas>();
        assert!(
            atlas.is_ready(),
            "config-file search paths should load the pack"
        );
    }
}
