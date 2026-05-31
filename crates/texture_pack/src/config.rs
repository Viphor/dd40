//! [`TexturePackConfig`] — runtime knobs for the pack loader.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::ecs::resource::Resource;
use dd40_texture_core::RenderLayer;

/// Configuration for [`TexturePackPlugin`](crate::TexturePackPlugin).
///
/// Inserted as a [`Resource`] before [`TexturePackPlugin`] runs.  The
/// plugin reads it once at startup; mutating it after the atlas is
/// built has no effect until a future reload feature is added.
///
/// # Fields
///
/// - `search_paths`: ordered list of pack-root directories to scan.
///   Each directory is expected to contain an
///   `assets/<namespace>/textures/block/**/*.png` hierarchy.
///   **Later paths override earlier ones** when the same key appears
///   in more than one pack — this is the simple analogue of
///   Minecraft's pack-priority list.
/// - `classification_overrides`: forces specific texture keys into a
///   particular [`RenderLayer`], bypassing the alpha-histogram
///   classifier.  Useful for, e.g., a solid-coloured glass that
///   should still render translucent.
#[derive(Resource, Debug, Clone, Default)]
pub struct TexturePackConfig {
    /// Ordered list of pack-root directories.  Last entry wins on key
    /// collision.
    pub search_paths: Vec<PathBuf>,
    /// Per-key render-layer overrides.  Keys are
    /// `"<namespace>:block/<name>"`.
    pub classification_overrides: HashMap<String, RenderLayer>,
}

impl TexturePackConfig {
    /// Shortcut for the common single-pack case.
    pub fn with_search_path<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            search_paths: vec![path.into()],
            classification_overrides: HashMap::new(),
        }
    }

    /// Appends an override for one texture key.
    pub fn with_override<S: Into<String>>(mut self, key: S, layer: RenderLayer) -> Self {
        self.classification_overrides.insert(key.into(), layer);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_search_path_has_one_entry() {
        let cfg = TexturePackConfig::with_search_path("foo");
        assert_eq!(cfg.search_paths, vec![PathBuf::from("foo")]);
        assert!(cfg.classification_overrides.is_empty());
    }

    #[test]
    fn override_builder_records_entry() {
        let cfg = TexturePackConfig::default().with_override("ns:block/x", RenderLayer::Cutout);
        assert_eq!(
            cfg.classification_overrides.get("ns:block/x"),
            Some(&RenderLayer::Cutout)
        );
    }
}
