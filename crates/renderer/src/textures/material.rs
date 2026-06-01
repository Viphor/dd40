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
/// field order, types, and padding in sync.  The struct is 64 bytes
/// (four std140 vec4 slots).
#[derive(Debug, Clone, Copy, ShaderType)]
#[repr(C)]
pub struct BlockAtlasParams {
    /// Array layer for the base texture.
    pub layer: u32,
    /// Alpha cutoff for the cutout pass; 0.0 for opaque/translucent so
    /// the shader's `discard` branch never fires.
    pub alpha_cutoff: f32,
    /// Non-zero if the per-vertex colour should be multiplied into
    /// the sampled texel (used by leaves / water);
    /// zero to show the texture as authored.  `u32` rather than
    /// `bool` because WGSL has no `bool` uniform type.
    pub tinted: u32,
    /// Non-zero if the material should sample the overlay layer and
    /// composite it on top of the base.  Mirrors
    /// `BucketKey::Static::overlay_layer.is_some()`.
    pub has_overlay: u32,
    /// Array layer for the overlay texture; ignored when
    /// `has_overlay == 0`.
    pub overlay_layer: u32,
    /// Padding to the next 16-byte boundary.  Three `u32`s of slack
    /// for future flags (animation frame, frame count, …).
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
    /// Atlas sub-rect minimum for the base texture (in normalised
    /// atlas-layer UV space).  The shader wraps the per-vertex
    /// tile-space UV via `fract` then maps it back into this rect:
    /// `atlas_uv = uv_min + fract(in.uv) * uv_size`.  This is what
    /// makes greedy-merged quads *tile* the texture per block instead
    /// of stretching it across the merged extent.
    pub uv_min: Vec2,
    /// Atlas sub-rect size for the base texture.
    pub uv_size: Vec2,
    /// Atlas sub-rect minimum for the overlay texture.  Ignored when
    /// `has_overlay == 0`.
    pub overlay_uv_min: Vec2,
    /// Atlas sub-rect size for the overlay texture.
    pub overlay_uv_size: Vec2,
}

impl Default for BlockAtlasParams {
    fn default() -> Self {
        Self {
            layer: 0,
            alpha_cutoff: 0.0,
            tinted: 0,
            has_overlay: 0,
            overlay_layer: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
            uv_min: Vec2::ZERO,
            uv_size: Vec2::ONE,
            overlay_uv_min: Vec2::ZERO,
            overlay_uv_size: Vec2::ONE,
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
    ///
    /// `overlay_layer = Some(_)` activates the overlay-compositing
    /// branch in the shader; `None` disables it.
    pub fn for_layer(
        atlas: Handle<Image>,
        atlas_layer: u32,
        render_layer: RenderLayer,
        tinted: bool,
        overlay_layer: Option<u32>,
        uv_min: Vec2,
        uv_size: Vec2,
        overlay_uv_min: Vec2,
        overlay_uv_size: Vec2,
    ) -> Self {
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
                tinted: u32::from(tinted),
                has_overlay: u32::from(overlay_layer.is_some()),
                overlay_layer: overlay_layer.unwrap_or(0),
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
                uv_min,
                uv_size,
                overlay_uv_min,
                overlay_uv_size,
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

    fn mk(layer: u32, rl: RenderLayer, tinted: bool, overlay: Option<u32>) -> BlockAtlasMaterial {
        BlockAtlasMaterial::for_layer(
            Handle::default(),
            layer,
            rl,
            tinted,
            overlay,
            Vec2::ZERO,
            Vec2::ONE,
            Vec2::ZERO,
            Vec2::ONE,
        )
    }

    #[test]
    fn for_layer_opaque_sets_opaque_alpha_mode() {
        let m = mk(5, RenderLayer::Opaque, false, None);
        assert!(matches!(m.alpha_mode, AlphaMode::Opaque));
        assert_eq!(m.params.layer, 5);
        assert_eq!(m.params.alpha_cutoff, 0.0);
        assert_eq!(m.params.tinted, 0);
        assert_eq!(m.params.has_overlay, 0);
    }

    #[test]
    fn for_layer_cutout_sets_mask_alpha_mode_with_cutoff() {
        let m = mk(3, RenderLayer::Cutout, false, None);
        assert!(matches!(m.alpha_mode, AlphaMode::Mask(c) if (c - 0.5).abs() < 1e-6));
        assert_eq!(m.params.alpha_cutoff, 0.5);
    }

    #[test]
    fn for_layer_translucent_sets_blend_alpha_mode() {
        let m = mk(7, RenderLayer::Translucent, false, None);
        assert!(matches!(m.alpha_mode, AlphaMode::Blend));
        assert_eq!(m.params.layer, 7);
        assert_eq!(m.params.alpha_cutoff, 0.0);
    }

    #[test]
    fn for_layer_tinted_true_packs_into_params() {
        assert_eq!(mk(0, RenderLayer::Opaque, false, None).params.tinted, 0);
        assert_eq!(mk(0, RenderLayer::Opaque, true, None).params.tinted, 1);
    }

    #[test]
    fn for_layer_overlay_some_packs_layer_and_flag() {
        let no_overlay = mk(1, RenderLayer::Opaque, false, None);
        let with_overlay = mk(1, RenderLayer::Opaque, false, Some(7));
        assert_eq!(no_overlay.params.has_overlay, 0);
        assert_eq!(no_overlay.params.overlay_layer, 0);
        assert_eq!(with_overlay.params.has_overlay, 1);
        assert_eq!(with_overlay.params.overlay_layer, 7);
    }

    #[test]
    fn for_layer_packs_atlas_sub_rects() {
        let m = BlockAtlasMaterial::for_layer(
            Handle::default(),
            0,
            RenderLayer::Opaque,
            false,
            Some(1),
            Vec2::new(0.25, 0.5),
            Vec2::new(0.125, 0.125),
            Vec2::new(0.75, 0.0),
            Vec2::new(0.125, 0.25),
        );
        assert_eq!(m.params.uv_min, Vec2::new(0.25, 0.5));
        assert_eq!(m.params.uv_size, Vec2::new(0.125, 0.125));
        assert_eq!(m.params.overlay_uv_min, Vec2::new(0.75, 0.0));
        assert_eq!(m.params.overlay_uv_size, Vec2::new(0.125, 0.25));
    }
}
