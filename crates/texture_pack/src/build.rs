//! Assembles decoded frames into a 2D-array `Image` and an
//! installable [`BlockAtlasSource`].
//!
//! The output of this stage is what the renderer actually samples.
//! Pixels are laid out as RGBA8 in a `Vec<u8>` of size
//! `layers * height * width * 4`, with layers stacked sequentially —
//! the layout Bevy's [`Image`] expects when
//! `TextureViewDimension::D2Array` is used.
//!
//! Smaller tiles are nearest-neighbour upscaled to match
//! [`AtlasLayout::tile_size`].  We use nearest-neighbour (not bilinear)
//! to preserve crisp pixel-art edges, matching Minecraft's behaviour.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::image::Image;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use dd40_texture_core::{AtlasId, BlockAtlasSource, RenderLayer, ResolvedTexture, TextureRef};

use crate::decode::DecodedTexture;
use crate::pack::AtlasLayout;

/// Built atlas: the pixel buffer + per-key resolved entries.
///
/// `build_pixels` produces the raw pixel `Vec<u8>`; the caller passes
/// it into [`Assets<Image>`] to get a [`Handle<Image>`], then calls
/// [`finalise`] to build the [`BlockAtlasSource`].
pub struct BuiltAtlas {
    /// Atlas identifier.  Always `AtlasId(0)` for this loader.
    pub id: AtlasId,
    /// Layout that produced the pixels.
    pub layout: AtlasLayout,
    /// Resolved entries keyed by texture name.
    pub entries: HashMap<String, ResolvedTexture>,
    /// Raw pixel data, ready for [`Image::new`].
    pub pixels: Vec<u8>,
}

/// Builds the pixel buffer for the given layout + decoded textures.
///
/// Returns the buffer alongside the per-key entry map.  This function
/// has no Bevy dependency beyond [`AtlasId`] etc. so it can be unit-
/// tested without a [`World`].
pub fn build_pixels(id: AtlasId, layout: AtlasLayout, decoded: &[DecodedTexture]) -> BuiltAtlas {
    let layer_pixels = (layout.width() as usize) * (layout.height() as usize) * 4;
    let mut pixels = vec![0u8; layer_pixels * layout.layers as usize];

    let mut entries: HashMap<String, ResolvedTexture> = HashMap::new();

    for (placement, tex) in layout.placements.iter().zip(decoded.iter()) {
        debug_assert_eq!(placement.key, tex.key);

        for (frame_idx, frame) in tex.frames.iter().enumerate() {
            let layer = placement.base_layer as usize + frame_idx;
            let layer_offset = layer * layer_pixels;
            blit_tile(
                &mut pixels[layer_offset..layer_offset + layer_pixels],
                layout.width() as usize,
                placement.col as usize,
                placement.row as usize,
                layout.tile_size as usize,
                frame,
                tex.tile_size as usize,
            );
        }

        let uv = placement.to_uv(&layout);
        entries.insert(
            placement.key.clone(),
            ResolvedTexture {
                atlas: id,
                uv,
                render_layer: tex.render_layer,
                animation: tex.animation.clone(),
            },
        );
    }

    BuiltAtlas {
        id,
        layout,
        entries,
        pixels,
    }
}

/// Constructs a Bevy 2D-array [`Image`] from a [`BuiltAtlas`].
///
/// The returned image has `D2Array` view dimension and `Rgba8UnormSrgb`
/// format — the renderer's WGSL samples `texture_2d_array<f32>`.
pub fn build_image(atlas: &BuiltAtlas) -> Image {
    let extent = Extent3d {
        width: atlas.layout.width(),
        height: atlas.layout.height(),
        depth_or_array_layers: atlas.layout.layers,
    };
    let mut image = Image::new(
        extent,
        TextureDimension::D2,
        atlas.pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        label: Some("dd40_block_atlas_array_view"),
        dimension: Some(TextureViewDimension::D2Array),
        ..Default::default()
    });
    image
}

/// Concrete [`BlockAtlasSource`] backed by a hash-map and a single
/// image handle.
#[derive(Debug, Clone)]
pub struct StaticBlockAtlasSource {
    id: AtlasId,
    entries: HashMap<String, ResolvedTexture>,
    handle: Handle<Image>,
}

impl StaticBlockAtlasSource {
    /// Wraps a built atlas with its uploaded image handle.
    pub fn new(atlas: BuiltAtlas, handle: Handle<Image>) -> Self {
        Self {
            id: atlas.id,
            entries: atlas.entries,
            handle,
        }
    }
}

impl BlockAtlasSource for StaticBlockAtlasSource {
    fn resolve(&self, r: &TextureRef) -> Option<ResolvedTexture> {
        match r {
            TextureRef::Named(name) => self.entries.get(name).cloned(),
            TextureRef::Direct { atlas, uv } if *atlas == self.id => Some(ResolvedTexture {
                atlas: *atlas,
                uv: *uv,
                render_layer: RenderLayer::Opaque,
                animation: None,
            }),
            TextureRef::Direct { .. } => None,
        }
    }
    fn texture(&self, atlas: AtlasId) -> Option<Handle<Image>> {
        (atlas == self.id).then(|| self.handle.clone())
    }
}

/// End-to-end one-call helper: builds pixels, uploads an Image, and
/// returns an `Arc<dyn BlockAtlasSource>` ready for
/// [`BlockAtlas::set_source`].
pub fn install(
    layout: AtlasLayout,
    decoded: &[DecodedTexture],
    images: &mut Assets<Image>,
) -> (Arc<dyn BlockAtlasSource>, AtlasId) {
    let id = AtlasId(0);
    let built = build_pixels(id, layout, decoded);
    let image = build_image(&built);
    let handle = images.add(image);
    let source = StaticBlockAtlasSource::new(built, handle);
    (Arc::new(source), id)
}

fn blit_tile(
    layer_pixels: &mut [u8],
    layer_width_px: usize,
    cell_col: usize,
    cell_row: usize,
    cell_size_px: usize,
    src: &[u8],
    src_size_px: usize,
) {
    let dst_x = cell_col * cell_size_px;
    let dst_y = cell_row * cell_size_px;

    if src_size_px == cell_size_px {
        for row in 0..cell_size_px {
            let dst_off = ((dst_y + row) * layer_width_px + dst_x) * 4;
            let src_off = row * src_size_px * 4;
            layer_pixels[dst_off..dst_off + cell_size_px * 4]
                .copy_from_slice(&src[src_off..src_off + cell_size_px * 4]);
        }
    } else {
        // Nearest-neighbour upscale.
        for row in 0..cell_size_px {
            let sy = (row * src_size_px) / cell_size_px;
            for col in 0..cell_size_px {
                let sx = (col * src_size_px) / cell_size_px;
                let src_off = (sy * src_size_px + sx) * 4;
                let dst_off = ((dst_y + row) * layer_width_px + dst_x + col) * 4;
                layer_pixels[dst_off..dst_off + 4].copy_from_slice(&src[src_off..src_off + 4]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::compute_layout;
    use dd40_texture_core::{AnimationSpec, AtlasUv, RenderLayer};

    fn solid(key: &str, tile: u32, fill: [u8; 4], frames: usize) -> DecodedTexture {
        let frame = vec![fill; (tile * tile) as usize]
            .into_iter()
            .flatten()
            .collect();
        DecodedTexture {
            key: key.to_owned(),
            tile_size: tile,
            frames: vec![frame; frames],
            render_layer: RenderLayer::Opaque,
            animation: None,
        }
    }

    #[test]
    fn empty_input_produces_empty_atlas() {
        let (id, layout) = compute_layout(&[]);
        let built = build_pixels(id, layout, &[]);
        assert!(built.entries.is_empty());
        // 1x1x1 layer of RGBA zeros.
        assert_eq!(built.pixels.len(), 4);
    }

    #[test]
    fn two_textures_get_placed_in_their_cells() {
        let inputs = vec![
            solid("a", 2, [255, 0, 0, 255], 1),
            solid("b", 2, [0, 255, 0, 255], 1),
        ];
        let (id, layout) = compute_layout(&inputs);
        let built = build_pixels(id, layout, &inputs);

        let entry_a = built.entries.get("a").unwrap();
        let entry_b = built.entries.get("b").unwrap();
        assert!(entry_a.uv.min.x < entry_b.uv.min.x || entry_a.uv.min.y < entry_b.uv.min.y);

        // First pixel of cell (0,0) layer 0 is red.
        assert_eq!(&built.pixels[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn animated_texture_writes_consecutive_layers() {
        let mut anim = solid("water", 2, [0, 0, 255, 255], 1);
        anim.frames = vec![vec![10; 16], vec![20; 16], vec![30; 16]];
        anim.animation = Some(AnimationSpec::linear(3, 50));
        let inputs = vec![anim];
        let (id, layout) = compute_layout(&inputs);
        assert_eq!(layout.layers, 3);
        let built = build_pixels(id, layout, &inputs);

        let layer_bytes = (built.layout.width() as usize) * (built.layout.height() as usize) * 4;
        assert_eq!(built.pixels[0], 10);
        assert_eq!(built.pixels[layer_bytes], 20);
        assert_eq!(built.pixels[2 * layer_bytes], 30);

        let entry = built.entries.get("water").unwrap();
        assert!(entry.animation.is_some());
        assert_eq!(entry.uv.base_layer, 0);
    }

    #[test]
    fn smaller_tile_is_nearest_neighbour_upscaled() {
        let big = solid("big", 4, [0, 0, 0, 255], 1);
        let small = solid("small", 2, [255, 255, 255, 255], 1);
        let inputs = vec![big, small];
        let (id, layout) = compute_layout(&inputs);
        assert_eq!(layout.tile_size, 4);
        let built = build_pixels(id, layout, &inputs);

        // 2x2 grid; "small" is at cell (1,0). Its top-left pixel
        // (layer 0, x=4, y=0) should be white after upscaling.
        let off = 4 * 4; // y=0, x=4
        assert_eq!(&built.pixels[off..off + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn source_resolves_named_and_direct_refs() {
        let inputs = vec![solid("ns:block/x", 2, [1, 2, 3, 255], 1)];
        let (id, layout) = compute_layout(&inputs);
        let built = build_pixels(id, layout, &inputs);
        let source = StaticBlockAtlasSource::new(built, Handle::default());

        let named = source.resolve(&TextureRef::named("ns:block/x")).unwrap();
        assert_eq!(named.atlas, AtlasId(0));

        assert!(source.resolve(&TextureRef::named("nope")).is_none());

        let direct_known = source.resolve(&TextureRef::Direct {
            atlas: AtlasId(0),
            uv: AtlasUv::full_layer(0),
        });
        assert!(direct_known.is_some());

        let direct_unknown = source.resolve(&TextureRef::Direct {
            atlas: AtlasId(42),
            uv: AtlasUv::full_layer(0),
        });
        assert!(direct_unknown.is_none());
    }
}
