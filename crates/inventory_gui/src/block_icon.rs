//! Procedural isometric-cube block icons.
//!
//! Renders a Minecraft-style three-face block preview into a Bevy
//! [`Image`] asset.  Used by [`crate::icons::ItemIconCache`] as the
//! placeable-block fallback when the item has no PNG icon.
//!
//! The output image is `ICON_SIZE × ICON_SIZE` pixels with transparent
//! background.  Three rhombic faces are drawn at descending brightness:
//!
//! - **Top face**   — base colour at [`TOP_BRIGHTNESS`].
//! - **Left face**  — base colour at [`LEFT_BRIGHTNESS`].
//! - **Right face** — base colour at [`RIGHT_BRIGHTNESS`].
//!
//! Pixel coverage uses barycentric coordinates against each face's
//! parallelogram, so no scanline rasterizer is needed.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

/// Edge length of the generated icon in pixels.
pub const ICON_SIZE: u32 = 64;

/// Brightness multiplier for the top face of the cube.
pub const TOP_BRIGHTNESS: f32 = 1.0;
/// Brightness multiplier for the left face of the cube.
pub const LEFT_BRIGHTNESS: f32 = 0.78;
/// Brightness multiplier for the right face of the cube.
pub const RIGHT_BRIGHTNESS: f32 = 0.58;

/// Rendering data for one face of the textured isometric icon.
///
/// All slices are borrowed from the caller's pixel buffers; the lifetimes
/// are tied to the call to [`build_block_icon_textured`].
pub struct TileFace<'a> {
    /// RGBA8 pixel buffer of `w × h × 4` bytes.
    pub pixels: &'a [u8],
    pub w: u32,
    pub h: u32,
    /// Optional RGB tint multiplied channel-by-channel into the base texels.
    /// Set from [`dd40_texture_core::BlockTextures::tinted_for`] + block colour.
    pub base_tint: Option<[u8; 3]>,
    /// Optional overlay pixels of the same `w × h` dimensions.
    /// Alpha-composited on top of the tinted base.
    pub overlay: Option<(&'a [u8], u32, u32)>,
    /// RGB tint applied to the overlay before compositing.  Usually the block
    /// colour — overlays (grass-side etc.) are always colour-tinted.
    pub overlay_tint: Option<[u8; 3]>,
}

/// Generates an isometric block icon for a single base colour.
pub fn build_block_icon(base: Color) -> Image {
    let size = ICON_SIZE;
    let w = size as f32;
    let mut data = vec![0u8; (size * size * 4) as usize];

    let top_apex = (w * 0.5, 0.0);
    let upper_right = (w, w * 0.25);
    let right = (w, w * 0.75);
    let left = (0.0, w * 0.75);
    let upper_left = (0.0, w * 0.25);
    let center = (w * 0.5, w * 0.5);

    fill_parallelogram(
        &mut data, size, top_apex,
        sub(upper_right, top_apex), sub(upper_left, top_apex),
        shade(base, TOP_BRIGHTNESS),
    );
    fill_parallelogram(
        &mut data, size, upper_left,
        sub(center, upper_left), sub(left, upper_left),
        shade(base, LEFT_BRIGHTNESS),
    );
    fill_parallelogram(
        &mut data, size, upper_right,
        sub(right, upper_right), sub(center, upper_right),
        shade(base, RIGHT_BRIGHTNESS),
    );

    new_image(size, data)
}

/// Generates an isometric block icon from actual texture tiles.
///
/// Each face argument carries the tile pixels, optional base tint, optional
/// overlay, and optional overlay tint.  Faces that are `None` fall back to a
/// flat colour derived from `base`.
pub fn build_block_icon_textured(
    top: Option<TileFace<'_>>,
    left: Option<TileFace<'_>>,
    right: Option<TileFace<'_>>,
    base: Color,
) -> Image {
    let size = ICON_SIZE;
    let w = size as f32;
    let mut data = vec![0u8; (size * size * 4) as usize];

    let top_apex = (w * 0.5, 0.0);
    let upper_right = (w, w * 0.25);
    let right_pt = (w, w * 0.75);
    let left_pt = (0.0, w * 0.75);
    let upper_left = (0.0, w * 0.25);
    let center = (w * 0.5, w * 0.5);

    draw_face(
        &mut data, size, top_apex,
        sub(upper_right, top_apex), sub(upper_left, top_apex),
        top, shade(base, TOP_BRIGHTNESS), TOP_BRIGHTNESS,
    );
    draw_face(
        &mut data, size, upper_left,
        sub(center, upper_left), sub(left_pt, upper_left),
        left, shade(base, LEFT_BRIGHTNESS), LEFT_BRIGHTNESS,
    );
    draw_face(
        &mut data, size,
        center,
        sub(upper_right, center),
        sub(right_pt, upper_right),
        right, shade(base, RIGHT_BRIGHTNESS), RIGHT_BRIGHTNESS,
    );

    new_image(size, data)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn new_image(size: u32, data: Vec<u8>) -> Image {
    Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn sub(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    (a.0 - b.0, a.1 - b.1)
}

fn shade(base: Color, factor: f32) -> [u8; 4] {
    let s = base.to_srgba();
    let b = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    [b(s.red * factor), b(s.green * factor), b(s.blue * factor), b(s.alpha)]
}

/// Nearest-neighbour sample from a flat RGBA8 tile.
fn sample_tile(pixels: &[u8], tile_w: u32, tile_h: u32, u: f32, v: f32) -> [u8; 4] {
    let x = ((u * tile_w as f32) as u32).min(tile_w.saturating_sub(1)) as usize;
    let y = ((v * tile_h as f32) as u32).min(tile_h.saturating_sub(1)) as usize;
    let idx = (y * tile_w as usize + x) * 4;
    if idx + 4 > pixels.len() {
        return [255, 0, 255, 255];
    }
    [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
}

/// Multiplies the RGB channels of `pixel` by `tint` (channel-by-channel u8 mul).
fn apply_rgb_tint([r, g, b, a]: [u8; 4], [tr, tg, tb]: [u8; 3]) -> [u8; 4] {
    [
        (r as u32 * tr as u32 / 255) as u8,
        (g as u32 * tg as u32 / 255) as u8,
        (b as u32 * tb as u32 / 255) as u8,
        a,
    ]
}

/// Dispatch to the textured or flat-colour parallelogram filler.
fn draw_face(
    data: &mut [u8],
    size: u32,
    origin: (f32, f32),
    edge_u: (f32, f32),
    edge_v: (f32, f32),
    face: Option<TileFace<'_>>,
    fallback: [u8; 4],
    brightness: f32,
) {
    match face {
        Some(f) => fill_parallelogram_face(data, size, origin, edge_u, edge_v, &f, brightness),
        None => fill_parallelogram(data, size, origin, edge_u, edge_v, fallback),
    }
}

fn fill_parallelogram(
    data: &mut [u8],
    size: u32,
    origin: (f32, f32),
    edge_u: (f32, f32),
    edge_v: (f32, f32),
    color: [u8; 4],
) {
    let det = edge_u.0 * edge_v.1 - edge_u.1 * edge_v.0;
    if det.abs() < 1e-6 { return; }
    let inv = 1.0 / det;
    for py in 0..size {
        for px in 0..size {
            let dx = px as f32 + 0.5 - origin.0;
            let dy = py as f32 + 0.5 - origin.1;
            let u = (dx * edge_v.1 - dy * edge_v.0) * inv;
            let v = (edge_u.0 * dy - edge_u.1 * dx) * inv;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                let idx = ((py * size + px) * 4) as usize;
                data[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

/// Textured parallelogram fill with tint and overlay compositing.
fn fill_parallelogram_face(
    data: &mut [u8],
    size: u32,
    origin: (f32, f32),
    edge_u: (f32, f32),
    edge_v: (f32, f32),
    face: &TileFace<'_>,
    brightness: f32,
) {
    let det = edge_u.0 * edge_v.1 - edge_u.1 * edge_v.0;
    if det.abs() < 1e-6 { return; }
    let inv = 1.0 / det;
    let apply_br = |c: u8| (c as f32 * brightness).clamp(0.0, 255.0) as u8;

    for py in 0..size {
        for px in 0..size {
            let dx = px as f32 + 0.5 - origin.0;
            let dy = py as f32 + 0.5 - origin.1;
            let u = (dx * edge_v.1 - dy * edge_v.0) * inv;
            let v = (edge_u.0 * dy - edge_u.1 * dx) * inv;
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }

            // --- Base texel ---
            let base_raw = sample_tile(face.pixels, face.w, face.h, u, v);
            if base_raw[3] == 0 { continue; }

            let tinted = if let Some(t) = face.base_tint {
                apply_rgb_tint(base_raw, t)
            } else {
                base_raw
            };
            let [mut r, mut g, mut b, mut a] = tinted;
            r = apply_br(r);
            g = apply_br(g);
            b = apply_br(b);

            // --- Overlay compositing ---
            if let Some((ov_pix, ov_w, ov_h)) = face.overlay {
                let ov_raw = sample_tile(ov_pix, ov_w, ov_h, u, v);
                if ov_raw[3] > 0 {
                    let ov_tinted = if let Some(t) = face.overlay_tint {
                        apply_rgb_tint(ov_raw, t)
                    } else {
                        ov_raw
                    };
                    let [or, og, ob, oa] = [
                        apply_br(ov_tinted[0]),
                        apply_br(ov_tinted[1]),
                        apply_br(ov_tinted[2]),
                        ov_tinted[3],
                    ];
                    let af = oa as f32 / 255.0;
                    r = (or as f32 * af + r as f32 * (1.0 - af)) as u8;
                    g = (og as f32 * af + g as f32 * (1.0 - af)) as u8;
                    b = (ob as f32 * af + b as f32 * (1.0 - af)) as u8;
                    a = oa.max(a);
                }
            }

            let idx = ((py * size + px) * 4) as usize;
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = a;
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let data = image.data.as_ref().expect("image has data");
        let idx = ((y * ICON_SIZE + x) * 4) as usize;
        [data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]
    }

    #[test]
    fn corner_pixels_are_transparent() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        for (x, y) in [
            (0, 0),
            (ICON_SIZE - 1, 0),
            (0, ICON_SIZE - 1),
            (ICON_SIZE - 1, ICON_SIZE - 1),
        ] {
            assert_eq!(pixel(&img, x, y)[3], 0, "corner ({x},{y}) must be transparent");
        }
    }

    #[test]
    fn center_pixel_is_opaque() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        let p = pixel(&img, ICON_SIZE / 2, ICON_SIZE / 4);
        assert_eq!(p[3], 255, "top-face centre must be opaque");
    }

    #[test]
    fn top_face_is_brighter_than_right_face() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        let top = pixel(&img, ICON_SIZE / 2, ICON_SIZE / 4);
        let right = pixel(&img, ICON_SIZE * 3 / 4, ICON_SIZE * 5 / 8);
        assert!(top[0] > right[0], "top R ({}) should exceed right R ({})", top[0], right[0]);
    }

    #[test]
    fn left_face_is_brighter_than_right_face() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        let left = pixel(&img, ICON_SIZE / 4, ICON_SIZE * 5 / 8);
        let right = pixel(&img, ICON_SIZE * 3 / 4, ICON_SIZE * 5 / 8);
        assert!(left[0] > right[0], "left R ({}) should exceed right R ({})", left[0], right[0]);
    }
}
