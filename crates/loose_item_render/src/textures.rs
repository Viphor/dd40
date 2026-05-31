//! Optional atlas-aware textured-cube scaffold.
//!
//! Compiled only with the `textures` Cargo feature.  Today this module
//! is a placeholder — the existing single-colour spinning cube in
//! [`crate::plugin`] remains the only visual.  When the renderer's
//! `BlockAtlasMaterial` lands (renderer-wgsl / renderer-mesh-split /
//! renderer-merge-key todos), this module will grow:
//!
//! 1. A startup system that builds a 6-face cube mesh with
//!    [`Mesh::ATTRIBUTE_UV_0`] and the per-vertex
//!    `i_atlas_layer` attribute, ready to be paired with
//!    `BlockAtlasMaterial`.
//! 2. An `attach_visuals` variant that, for placeable items with
//!    [`BlockTextures`] attached, applies the atlas material per face
//!    from [`BlockAtlas::resolve`].
//! 3. A fallback rule: items without `BlockTextures` (or while the
//!    atlas is loading) keep the colour-only [`StandardMaterial`]
//!    cube.
//!
//! [`BlockTextures`]: dd40_texture_core::BlockTextures
//! [`BlockAtlas`]: dd40_texture_core::BlockAtlas
//! [`BlockAtlas::resolve`]: dd40_texture_core::BlockAtlas::resolve

use bevy::prelude::*;

/// Marker resource indicating the textured-cube pipeline is wired in.
///
/// Inserted by [`crate::plugin::LooseItemRenderPlugin`] when the
/// `textures` feature is enabled.  Reserved for future use by
/// downstream tooling that wants to inspect whether textured loose
/// items are active.
#[derive(Resource, Default, Debug)]
pub struct TexturedCubeScaffold;

/// Returns `true` once the textured-cube pipeline can serve real
/// atlas-backed visuals.  Today always `false`; will be wired to
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
