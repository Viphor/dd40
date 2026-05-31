//! Atlas layout computation.
//!
//! Takes a slice of [`DecodedTexture`]s and computes:
//! - `tile_size` — the uniform side length every cell will be padded
//!   to (the max of all decoded tile sizes).
//! - `cols` × `rows` grid covering every texture.
//! - `layers` — how many array layers the atlas needs (max
//!   `animation.frame_count` across all textures, at least 1).
//! - A [`TilePlacement`] per texture describing its cell coordinates
//!   and base layer.
//!
//! Frames of an animated texture occupy **consecutive array layers**
//! at the same `(col, row)`; static textures occupy layer 0 only.
//! All tiles live at the same `(col, row)` across every layer, so a
//! single `AtlasUv` per texture covers every frame and the shader
//! only varies the `z` (layer) coordinate when sampling.

use bevy::math::Vec2;
use dd40_texture_core::{AtlasId, AtlasUv};

use crate::decode::DecodedTexture;

/// Where a single texture lives inside the packed atlas.
#[derive(Debug, Clone, PartialEq)]
pub struct TilePlacement {
    /// Same key the input [`DecodedTexture`] had.
    pub key: String,
    /// Grid column.
    pub col: u32,
    /// Grid row.
    pub row: u32,
    /// First array layer this texture's frames occupy.
    pub base_layer: u32,
    /// How many consecutive layers the frames span (≥ 1).
    pub frame_count: u32,
}

impl TilePlacement {
    /// Builds the [`AtlasUv`] for this placement given the atlas
    /// dimensions.  UVs are in `[0, 1]`.
    pub fn to_uv(&self, layout: &AtlasLayout) -> AtlasUv {
        let pixel_w = layout.tile_size as f32;
        let total_w = (layout.cols * layout.tile_size) as f32;
        let total_h = (layout.rows * layout.tile_size) as f32;
        let x = self.col as f32 * pixel_w;
        let y = self.row as f32 * pixel_w;
        AtlasUv {
            min: Vec2::new(x / total_w, y / total_h),
            max: Vec2::new((x + pixel_w) / total_w, (y + pixel_w) / total_h),
            base_layer: self.base_layer,
        }
    }
}

/// Result of [`compute_layout`].
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasLayout {
    /// Side length (px) of every uniform cell in the atlas.
    pub tile_size: u32,
    /// Number of columns.
    pub cols: u32,
    /// Number of rows.
    pub rows: u32,
    /// Number of array layers.
    pub layers: u32,
    /// Where each input texture was placed.
    pub placements: Vec<TilePlacement>,
}

impl AtlasLayout {
    /// Total width of one layer, in pixels.
    pub fn width(&self) -> u32 {
        self.cols * self.tile_size
    }
    /// Total height of one layer, in pixels.
    pub fn height(&self) -> u32 {
        self.rows * self.tile_size
    }
}

/// Computes a uniform-grid atlas layout for `textures`.
///
/// The grid is approximately square (`cols = ceil(sqrt(n))`, `rows =
/// ceil(n / cols)`).  Each tile is normalized to the maximum tile
/// size in the input set (smaller tiles are upscaled at fill time).
/// Empty input produces a 1×1×1 layout with no placements — the
/// caller should treat that as "no atlas available" and fall back
/// to colour rendering.
///
/// The single configurable [`AtlasId`] returned is always
/// `AtlasId(0)` — this loader produces exactly one atlas.  Future
/// loaders may produce more.
pub fn compute_layout(textures: &[DecodedTexture]) -> (AtlasId, AtlasLayout) {
    let n = textures.len() as u32;
    if n == 0 {
        return (
            AtlasId(0),
            AtlasLayout {
                tile_size: 1,
                cols: 1,
                rows: 1,
                layers: 1,
                placements: Vec::new(),
            },
        );
    }
    let tile_size = textures.iter().map(|t| t.tile_size).max().unwrap_or(1);
    let cols = (n as f32).sqrt().ceil() as u32;
    let rows = n.div_ceil(cols);
    let layers = textures
        .iter()
        .map(|t| t.frames.len() as u32)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut placements = Vec::with_capacity(textures.len());
    for (i, tex) in textures.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        placements.push(TilePlacement {
            key: tex.key.clone(),
            col,
            row,
            base_layer: 0,
            frame_count: tex.frames.len() as u32,
        });
    }

    (
        AtlasId(0),
        AtlasLayout {
            tile_size,
            cols,
            rows,
            layers,
            placements,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_texture_core::RenderLayer;

    fn fake(key: &str, tile: u32, frames: usize) -> DecodedTexture {
        DecodedTexture {
            key: key.to_owned(),
            tile_size: tile,
            frames: (0..frames)
                .map(|_| vec![0u8; (tile * tile * 4) as usize])
                .collect(),
            render_layer: RenderLayer::Opaque,
            animation: None,
        }
    }

    #[test]
    fn empty_input_yields_unit_layout() {
        let (_, layout) = compute_layout(&[]);
        assert_eq!(layout.cols, 1);
        assert_eq!(layout.rows, 1);
        assert_eq!(layout.layers, 1);
        assert!(layout.placements.is_empty());
    }

    #[test]
    fn single_static_texture_uses_one_cell_one_layer() {
        let (_, layout) = compute_layout(&[fake("k", 16, 1)]);
        assert_eq!(layout.cols, 1);
        assert_eq!(layout.rows, 1);
        assert_eq!(layout.layers, 1);
        assert_eq!(layout.tile_size, 16);
        assert_eq!(layout.placements.len(), 1);
        assert_eq!(layout.placements[0].col, 0);
        assert_eq!(layout.placements[0].row, 0);
    }

    #[test]
    fn grid_is_approximately_square() {
        let inputs: Vec<_> = (0..10).map(|i| fake(&format!("k{i}"), 4, 1)).collect();
        let (_, layout) = compute_layout(&inputs);
        // ceil(sqrt(10)) = 4 cols, ceil(10/4) = 3 rows.
        assert_eq!(layout.cols, 4);
        assert_eq!(layout.rows, 3);
        assert_eq!(layout.placements.len(), 10);
    }

    #[test]
    fn tile_size_is_max_of_inputs() {
        let inputs = vec![fake("a", 16, 1), fake("b", 32, 1)];
        let (_, layout) = compute_layout(&inputs);
        assert_eq!(layout.tile_size, 32);
    }

    #[test]
    fn layers_match_max_frame_count() {
        let inputs = vec![fake("static", 16, 1), fake("anim", 16, 4)];
        let (_, layout) = compute_layout(&inputs);
        assert_eq!(layout.layers, 4);
        assert_eq!(layout.placements[0].frame_count, 1);
        assert_eq!(layout.placements[1].frame_count, 4);
    }

    #[test]
    fn uv_covers_correct_cell() {
        let inputs: Vec<_> = (0..4).map(|i| fake(&format!("k{i}"), 16, 1)).collect();
        let (_, layout) = compute_layout(&inputs);
        // 4 tiles → 2×2 grid. UV for cell (1,0) = (0.5..1.0, 0..0.5).
        let uv = layout.placements[1].to_uv(&layout);
        assert!((uv.min.x - 0.5).abs() < 1e-6);
        assert!((uv.max.x - 1.0).abs() < 1e-6);
        assert!((uv.min.y - 0.0).abs() < 1e-6);
        assert!((uv.max.y - 0.5).abs() < 1e-6);
    }
}
