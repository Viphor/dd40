//! Textured-block rendering pipeline.
//!
//! Only compiled when the `textures` Cargo feature is enabled.
//!
//! This module owns:
//! - [`BlockAtlasMaterial`] — a Bevy [`Material`] that samples the
//!   2D-array texture installed in [`BlockAtlas`] at a layer index
//!   selected per-material instance.
//! - [`BlockAtlasMaterialPlugin`] — registers the three [`MaterialPlugin`]
//!   instances (one per [`RenderLayer`]) and the WGSL fragment shader.
//!
//! The greedy mesh / bucket split that *uses* these materials lands in
//! a follow-up commit; this commit puts the material in place so the
//! mesh code can rely on it existing.
//!
//! # Layer per material instance
//!
//! Faces in a chunk are grouped into buckets by `(render_layer, atlas_layer)`.
//! Each bucket gets one mesh and one [`BlockAtlasMaterial`] instance with
//! `params.layer` set accordingly.  This keeps the shader trivial — no
//! per-vertex layer attribute, no custom vertex shader — at the cost of
//! more material instances per chunk.  Typical worlds use a few dozen
//! textures, so the cost is acceptable.

use bevy::asset::uuid_handle;
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use dd40_texture_core::RenderLayer;

/// Weak handle to the block atlas fragment shader.
///
/// Loaded internally by [`BlockAtlasMaterialPlugin`].  Exposed as a
/// constant so tests and downstream code can reference the same
/// asset without going through `AssetServer::load` again.
pub const BLOCK_ATLAS_SHADER_HANDLE: Handle<bevy::shader::Shader> =
    uuid_handle!("8c7e1b35-3f3a-4d24-8a91-3e94a5e9b9d8");

/// GPU-side parameters for [`BlockAtlasMaterial`].
///
/// Mirrored exactly in `block_atlas.wgsl::BlockAtlasParams` — keep the
/// field order, types, and padding in sync.
#[derive(Debug, Clone, Copy, ShaderType)]
#[repr(C)]
pub struct BlockAtlasParams {
    /// Array layer this material samples from.
    pub layer: u32,
    /// Alpha cutoff for the cutout pass; 0.0 for opaque/translucent so
    /// the shader's `discard` branch never fires.
    pub alpha_cutoff: f32,
    /// Padding to align the uniform to 16 bytes.
    pub _pad0: f32,
    /// Padding to align the uniform to 16 bytes.
    pub _pad1: f32,
}

impl Default for BlockAtlasParams {
    fn default() -> Self {
        Self {
            layer: 0,
            alpha_cutoff: 0.0,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}

/// Custom material that samples a 2D-array atlas texture at a
/// material-instance-fixed array layer.
///
/// One instance of this material per `(RenderLayer, atlas_layer)`
/// bucket in a chunk.  The `alpha_mode` field decides which Bevy
/// pipeline (opaque / mask / blend) the bucket renders in.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BlockAtlasMaterial {
    /// The 2D-array atlas texture.  All instances share the same
    /// handle — this is the texture handle produced by
    /// [`BlockAtlas::texture`](dd40_texture_core::BlockAtlas::texture).
    #[texture(0, dimension = "2d_array")]
    #[sampler(1)]
    pub atlas: Handle<Image>,
    /// Per-instance layer + alpha cutoff.
    #[uniform(2)]
    pub params: BlockAtlasParams,
    /// Decides which Bevy render pipeline this material uses.
    pub alpha_mode: AlphaMode,
}

impl Material for BlockAtlasMaterial {
    fn fragment_shader() -> ShaderRef {
        BLOCK_ATLAS_SHADER_HANDLE.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

impl BlockAtlasMaterial {
    /// Convenience constructor that picks the right `AlphaMode` and
    /// `alpha_cutoff` for the given [`RenderLayer`].
    pub fn for_layer(atlas: Handle<Image>, atlas_layer: u32, render_layer: RenderLayer) -> Self {
        let (alpha_mode, alpha_cutoff) = match render_layer {
            RenderLayer::Opaque => (AlphaMode::Opaque, 0.0),
            RenderLayer::Cutout => (AlphaMode::Mask(0.5), 0.5),
            RenderLayer::Translucent => (AlphaMode::Blend, 0.0),
        };
        Self {
            atlas,
            params: BlockAtlasParams {
                layer: atlas_layer,
                alpha_cutoff,
                _pad0: 0.0,
                _pad1: 0.0,
            },
            alpha_mode,
        }
    }
}

/// Registers [`BlockAtlasMaterial`] with Bevy's [`MaterialPlugin`] and
/// embeds the fragment shader as a runtime asset.
///
/// Auto-added by [`crate::RendererPlugin`] when the `textures` feature
/// is enabled.
#[derive(Default)]
pub struct BlockAtlasMaterialPlugin;

impl Plugin for BlockAtlasMaterialPlugin {
    fn build(&self, app: &mut App) {
        // Embed the shader source so the binary ships standalone.
        // `assets/shaders/block_atlas.wgsl` is relative to the
        // renderer crate root and loaded once at startup.
        let source = include_str!("../../assets/shaders/block_atlas.wgsl");
        let shader = bevy::shader::Shader::from_wgsl(source, "block_atlas.wgsl");
        let _ = app
            .world_mut()
            .resource_mut::<Assets<bevy::shader::Shader>>()
            .insert(&BLOCK_ATLAS_SHADER_HANDLE, shader);

        app.add_plugins(MaterialPlugin::<BlockAtlasMaterial>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_layer_opaque_sets_opaque_alpha_mode() {
        let m = BlockAtlasMaterial::for_layer(Handle::default(), 5, RenderLayer::Opaque);
        assert!(matches!(m.alpha_mode, AlphaMode::Opaque));
        assert_eq!(m.params.layer, 5);
        assert_eq!(m.params.alpha_cutoff, 0.0);
    }

    #[test]
    fn for_layer_cutout_sets_mask_alpha_mode_with_cutoff() {
        let m = BlockAtlasMaterial::for_layer(Handle::default(), 3, RenderLayer::Cutout);
        assert!(matches!(m.alpha_mode, AlphaMode::Mask(c) if (c - 0.5).abs() < 1e-6));
        assert_eq!(m.params.alpha_cutoff, 0.5);
    }

    #[test]
    fn for_layer_translucent_sets_blend_alpha_mode() {
        let m = BlockAtlasMaterial::for_layer(Handle::default(), 7, RenderLayer::Translucent);
        assert!(matches!(m.alpha_mode, AlphaMode::Blend));
        assert_eq!(m.params.layer, 7);
        assert_eq!(m.params.alpha_cutoff, 0.0);
    }
}
