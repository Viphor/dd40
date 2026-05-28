//! Procedural isometric-cube block icons.
//!
//! Renders a Minecraft-style three-face block preview into a Bevy
//! [`Image`] asset.  Used by [`crate::icons::ItemIconCache`] as the
//! placeable-block fallback when the item has no PNG icon — replacing
//! the previous flat colour swatch.
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

/// Edge length of the generated icon in pixels.  Slot widgets scale
/// this down to their actual size; 64 keeps the icon crisp at both
/// [`crate::slot_widget::SLOT_SIZE`] (40 px) and
/// [`crate::slot_widget::GRID_SLOT_SIZE`] (56 px).
pub const ICON_SIZE: u32 = 64;

/// Brightness multiplier for the top face of the cube.
pub const TOP_BRIGHTNESS: f32 = 1.0;
/// Brightness multiplier for the left face of the cube.
pub const LEFT_BRIGHTNESS: f32 = 0.78;
/// Brightness multiplier for the right face of the cube.
pub const RIGHT_BRIGHTNESS: f32 = 0.58;

/// Generates an isometric block icon for a single base colour.
///
/// The returned [`Image`] is RGBA8-sRGB with a transparent background
/// outside the hexagonal silhouette.  Suitable for use as a UI
/// [`ImageNode`] texture.
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

    let top_color = shade(base, TOP_BRIGHTNESS);
    let left_color = shade(base, LEFT_BRIGHTNESS);
    let right_color = shade(base, RIGHT_BRIGHTNESS);

    fill_parallelogram(
        &mut data,
        size,
        top_apex,
        sub(upper_right, top_apex),
        sub(upper_left, top_apex),
        top_color,
    );
    fill_parallelogram(
        &mut data,
        size,
        upper_left,
        sub(center, upper_left),
        sub(left, upper_left),
        left_color,
    );
    fill_parallelogram(
        &mut data,
        size,
        upper_right,
        sub(right, upper_right),
        sub(center, upper_right),
        right_color,
    );

    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
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
    let srgba = base.to_srgba();
    let to_byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        to_byte(srgba.red * factor),
        to_byte(srgba.green * factor),
        to_byte(srgba.blue * factor),
        to_byte(srgba.alpha),
    ]
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
    if det.abs() < 1e-6 {
        return;
    }
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
            assert_eq!(
                pixel(&img, x, y)[3],
                0,
                "corner ({x},{y}) must be transparent"
            );
        }
    }

    #[test]
    fn center_pixel_is_opaque() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        // The geometric center sits on a shared edge between faces; the
        // top face occupies the row just above it, so sample y = H/4
        // (deep inside the top face).
        let p = pixel(&img, ICON_SIZE / 2, ICON_SIZE / 4);
        assert_eq!(p[3], 255, "top-face centre must be opaque");
    }

    #[test]
    fn top_face_is_brighter_than_right_face() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        let top = pixel(&img, ICON_SIZE / 2, ICON_SIZE / 4);
        let right = pixel(&img, ICON_SIZE * 3 / 4, ICON_SIZE * 5 / 8);
        assert!(
            top[0] > right[0],
            "top R ({}) should exceed right R ({})",
            top[0],
            right[0]
        );
    }

    #[test]
    fn left_face_is_brighter_than_right_face() {
        let img = build_block_icon(Color::srgb(0.8, 0.2, 0.2));
        let left = pixel(&img, ICON_SIZE / 4, ICON_SIZE * 5 / 8);
        let right = pixel(&img, ICON_SIZE * 3 / 4, ICON_SIZE * 5 / 8);
        assert!(
            left[0] > right[0],
            "left R ({}) should exceed right R ({})",
            left[0],
            right[0]
        );
    }
}
