//! Optional atlas-aware icon-resolution scaffold.
//!
//! Compiled only with the `textures` Cargo feature.  Today this module
//! is a placeholder — the existing procedural three-face cube
//! ([`crate::block_icon`]) remains the only icon renderer.  When the
//! renderer's `BlockAtlasMaterial` lands (renderer-wgsl /
//! renderer-mesh-split / renderer-merge-key todos), this module will
//! grow:
//!
//! 1. A system that reads each placeable item's [`BlockTextures`] from
//!    the [`BlockRegistry`], looks up the top-face UV in
//!    [`BlockAtlas::resolve`], and stamps an [`ImageNode`] slice of the
//!    underlying atlas image into [`crate::icons::ItemIconCache`].
//! 2. A fallback rule: when the block has no `BlockTextures` or the
//!    atlas is not yet ready (`BlockAtlas::is_loaded()` false), keep
//!    the procedural cube.
//! 3. 3D mini-cube icons sampling all three visible faces — a follow-up
//!    after the v1 flat top-face icon ships.
//!
//! The scaffold exists today so:
//! - downstream crates depending on this one with `--features textures`
//!   already pull in `dd40_texture_core` and don't have to flip a flag
//!   later;
//! - `TextureCorePlugin` is already auto-added by
//!   [`crate::plugin::InventoryGuiPlugin`] when the feature is on, so
//!   `BlockTextures` is a known [`dd40_core::block::data::BlockData`]
//!   type as soon as the GUI plugin is installed.
//!
//! [`BlockTextures`]: dd40_texture_core::BlockTextures
//! [`BlockAtlas`]: dd40_texture_core::BlockAtlas
//! [`BlockAtlas::resolve`]: dd40_texture_core::BlockAtlas::resolve
//! [`BlockRegistry`]: dd40_core::block::BlockRegistry

use bevy::prelude::*;

/// Marker resource indicating the textured-icon pipeline is wired in.
///
/// Inserted by [`crate::plugin::InventoryGuiPlugin`] when the
/// `textures` feature is enabled.  Reserved for future use by
/// downstream debug-UI tools that want to surface whether textured
/// icons are active.
#[derive(Resource, Default, Debug)]
pub struct TexturedIconScaffold;

/// Returns `true` once the textured-icon pipeline can serve real
/// atlas-backed icons.  Today always `false`; will be wired to
/// `BlockAtlas::is_loaded()` once the renderer pipeline lands.
pub fn pipeline_ready() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_is_not_ready_yet() {
        assert!(!pipeline_ready());
    }
}
