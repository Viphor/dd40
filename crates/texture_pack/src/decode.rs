//! PNG decoding, animation-strip splitting, and alpha-based
//! classification.
//!
//! Takes a [`DiscoveredTexture`] and produces a [`DecodedTexture`]:
//! one or more equally-sized RGBA8 frames plus a [`RenderLayer`].
//! No GPU work happens here — the result is plain `Vec<u8>` buffers
//! the upload stage will copy into the atlas texture.
//!
//! # Animation strip layout
//!
//! Minecraft encodes animation frames as a **vertical strip**: an
//! `N×(N*K)` PNG is `K` square frames of side `N`, stacked top-to-
//! bottom.  If the file has a `.mcmeta` describing animation, we
//! split on that.  If the file is square (`width == height`) and has
//! no `.mcmeta`, it's treated as a single static frame.  Anything
//! else (non-square without animation metadata) is rejected.
//!
//! # Classification
//!
//! Alpha histogram of all frames combined:
//! - Every pixel has `alpha == 255` → [`RenderLayer::Opaque`].
//! - Every pixel has `alpha == 0` or `alpha == 255`, with at least
//!   one of each → [`RenderLayer::Cutout`].
//! - Any pixel with `alpha` in `(0, 255)` → [`RenderLayer::Translucent`].
//!
//! Per-key overrides in [`TexturePackConfig::classification_overrides`]
//! take precedence over the histogram result.

use dd40_texture_core::{AnimationSpec, RenderLayer};

use crate::config::TexturePackConfig;
use crate::discover::DiscoveredTexture;
use crate::mcmeta::{McmetaError, parse_mcmeta};

/// Decoded, frame-split, classified texture ready for atlas packing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTexture {
    /// `"<ns>:block/<name>"` — same key the discovery stage produced.
    pub key: String,
    /// Side length of one frame in pixels.  All frames are square.
    pub tile_size: u32,
    /// RGBA8 frames, each `tile_size * tile_size * 4` bytes.
    pub frames: Vec<Vec<u8>>,
    /// Final render-layer assignment.
    pub render_layer: RenderLayer,
    /// Animation metadata if the source pack provided a `.mcmeta`
    /// with an `animation` block.
    pub animation: Option<AnimationSpec>,
}

/// Errors produced by [`decode`].
#[derive(Debug)]
pub enum DecodeError {
    /// PNG file could not be read or decoded.
    Image(image::ImageError),
    /// The `.mcmeta` file was unreadable or malformed.
    Mcmeta(McmetaError),
    /// The PNG had no animation metadata but wasn't square, so the
    /// frame layout is ambiguous.
    NonSquareWithoutAnimation { width: u32, height: u32 },
    /// `height` was not an integer multiple of `width`, so the
    /// vertical-strip split would produce non-square frames.
    StripNotDivisible { width: u32, height: u32 },
    /// Width was zero.
    ZeroSized,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image(e) => write!(f, "png decode error: {e}"),
            Self::Mcmeta(e) => write!(f, "mcmeta error: {e}"),
            Self::NonSquareWithoutAnimation { width, height } => {
                write!(f, "{width}x{height} png has no .mcmeta but is not square")
            }
            Self::StripNotDivisible { width, height } => write!(
                f,
                "animation strip {width}x{height} is not a whole number of {width}x{width} frames"
            ),
            Self::ZeroSized => write!(f, "png has zero width"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<image::ImageError> for DecodeError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<McmetaError> for DecodeError {
    fn from(value: McmetaError) -> Self {
        Self::Mcmeta(value)
    }
}

/// Decodes a single discovered texture from disk.
pub fn decode(
    discovered: &DiscoveredTexture,
    config: &TexturePackConfig,
) -> Result<DecodedTexture, DecodeError> {
    let img = image::open(&discovered.png_path)?.to_rgba8();
    let width = img.width();
    let height = img.height();
    if width == 0 || height == 0 {
        return Err(DecodeError::ZeroSized);
    }
    let pixels = img.into_raw();

    let frame_count_from_image = if height == width {
        1
    } else if height % width == 0 {
        height / width
    } else {
        return Err(DecodeError::StripNotDivisible { width, height });
    };

    let animation = match discovered.mcmeta_path.as_deref() {
        Some(path) => parse_mcmeta(path, frame_count_from_image)?,
        None => None,
    };

    let frame_count = match (&animation, frame_count_from_image) {
        (Some(a), _) => a.frame_count,
        (None, 1) => 1,
        (None, _) => {
            return Err(DecodeError::NonSquareWithoutAnimation { width, height });
        }
    };

    let frames = split_frames(&pixels, width, frame_count);
    let render_layer = config
        .classification_overrides
        .get(&discovered.key)
        .copied()
        .unwrap_or_else(|| classify_layer(&frames));

    Ok(DecodedTexture {
        key: discovered.key.clone(),
        tile_size: width,
        frames,
        render_layer,
        animation,
    })
}

/// Convenience: decodes every entry, collecting errors per-key so
/// one bad texture doesn't sink the whole pack.
pub fn decode_all(
    discovered: &[DiscoveredTexture],
    config: &TexturePackConfig,
) -> (Vec<DecodedTexture>, Vec<(String, DecodeError)>) {
    let mut ok = Vec::with_capacity(discovered.len());
    let mut errs = Vec::new();
    for d in discovered {
        match decode(d, config) {
            Ok(t) => ok.push(t),
            Err(e) => errs.push((d.key.clone(), e)),
        }
    }
    (ok, errs)
}

fn split_frames(pixels: &[u8], tile: u32, frame_count: u32) -> Vec<Vec<u8>> {
    let bytes_per_frame = (tile as usize) * (tile as usize) * 4;
    (0..frame_count as usize)
        .map(|i| {
            let start = i * bytes_per_frame;
            pixels[start..start + bytes_per_frame].to_vec()
        })
        .collect()
}

fn classify_layer(frames: &[Vec<u8>]) -> RenderLayer {
    let mut seen_zero = false;
    let mut seen_full = false;
    for frame in frames {
        for chunk in frame.chunks_exact(4) {
            let a = chunk[3];
            if a == 0 {
                seen_zero = true;
            } else if a == 255 {
                seen_full = true;
            } else {
                return RenderLayer::Translucent;
            }
        }
    }
    if seen_zero && seen_full {
        RenderLayer::Cutout
    } else if seen_full {
        RenderLayer::Opaque
    } else {
        // All-transparent texture; treat as cutout so it doesn't get
        // alpha-blended unnecessarily.
        RenderLayer::Cutout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::DiscoveredTexture;
    use image::{ImageBuffer, Rgba};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn write_png(path: &Path, width: u32, height: u32, fill: [u8; 4]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba(fill));
        buf.save(path).unwrap();
    }

    fn write_strip(path: &Path, tile: u32, frame_fills: &[[u8; 4]]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let height = tile * frame_fills.len() as u32;
        let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(tile, height);
        for (i, fill) in frame_fills.iter().enumerate() {
            for y in 0..tile {
                for x in 0..tile {
                    buf.put_pixel(x, i as u32 * tile + y, Rgba(*fill));
                }
            }
        }
        buf.save(path).unwrap();
    }

    fn discovered(key: &str, png: PathBuf, mcmeta: Option<PathBuf>) -> DiscoveredTexture {
        DiscoveredTexture {
            key: key.to_owned(),
            png_path: png,
            mcmeta_path: mcmeta,
            source_pack: PathBuf::from("/dev/null"),
        }
    }

    #[test]
    fn classifies_fully_opaque_solid_color() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stone.png");
        write_png(&path, 4, 4, [120, 120, 120, 255]);
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/stone", path, None);
        let decoded = decode(&d, &cfg).unwrap();
        assert_eq!(decoded.tile_size, 4);
        assert_eq!(decoded.frames.len(), 1);
        assert_eq!(decoded.render_layer, RenderLayer::Opaque);
        assert!(decoded.animation.is_none());
    }

    #[test]
    fn classifies_zero_or_full_alpha_as_cutout() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("leaves.png");
        // Half pixels transparent, half opaque.
        let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(2, 2);
        buf.put_pixel(0, 0, Rgba([0, 200, 0, 255]));
        buf.put_pixel(1, 0, Rgba([0, 0, 0, 0]));
        buf.put_pixel(0, 1, Rgba([0, 0, 0, 0]));
        buf.put_pixel(1, 1, Rgba([0, 200, 0, 255]));
        buf.save(&path).unwrap();
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/leaves", path, None);
        let decoded = decode(&d, &cfg).unwrap();
        assert_eq!(decoded.render_layer, RenderLayer::Cutout);
    }

    #[test]
    fn classifies_partial_alpha_as_translucent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("water.png");
        write_png(&path, 4, 4, [40, 80, 200, 128]);
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/water", path, None);
        let decoded = decode(&d, &cfg).unwrap();
        assert_eq!(decoded.render_layer, RenderLayer::Translucent);
    }

    #[test]
    fn override_beats_histogram() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stone.png");
        write_png(&path, 4, 4, [80, 80, 80, 255]);
        let cfg = TexturePackConfig::default()
            .with_override("minecraft:block/stone", RenderLayer::Cutout);
        let d = discovered("minecraft:block/stone", path, None);
        let decoded = decode(&d, &cfg).unwrap();
        assert_eq!(decoded.render_layer, RenderLayer::Cutout);
    }

    #[test]
    fn animated_strip_with_mcmeta_splits_into_frames() {
        let tmp = TempDir::new().unwrap();
        let png = tmp.path().join("water_flow.png");
        let mcmeta = tmp.path().join("water_flow.png.mcmeta");
        write_strip(
            &png,
            4,
            &[
                [10, 10, 200, 255],
                [20, 20, 210, 255],
                [30, 30, 220, 255],
                [40, 40, 230, 255],
            ],
        );
        std::fs::write(&mcmeta, br#"{ "animation": { "frametime": 2 } }"#).unwrap();
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/water_flow", png, Some(mcmeta));
        let decoded = decode(&d, &cfg).unwrap();
        assert_eq!(decoded.tile_size, 4);
        assert_eq!(decoded.frames.len(), 4);
        let anim = decoded.animation.unwrap();
        assert_eq!(anim.frame_count, 4);
        assert_eq!(anim.frame_indices, vec![0, 1, 2, 3]);
        // First pixel of frame 2 should be the third fill colour.
        assert_eq!(&decoded.frames[2][..4], &[30, 30, 220, 255]);
    }

    #[test]
    fn non_square_without_mcmeta_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bad.png");
        write_png(&path, 4, 8, [255, 255, 255, 255]);
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/bad", path, None);
        let err = decode(&d, &cfg).unwrap_err();
        assert!(matches!(
            err,
            DecodeError::NonSquareWithoutAnimation {
                width: 4,
                height: 8,
            }
        ));
    }

    #[test]
    fn strip_height_must_be_multiple_of_width() {
        let tmp = TempDir::new().unwrap();
        let png = tmp.path().join("bad_strip.png");
        let mcmeta = tmp.path().join("bad_strip.png.mcmeta");
        write_png(&png, 4, 10, [255, 0, 0, 255]);
        std::fs::write(&mcmeta, br#"{ "animation": {} }"#).unwrap();
        let cfg = TexturePackConfig::default();
        let d = discovered("minecraft:block/bad_strip", png, Some(mcmeta));
        let err = decode(&d, &cfg).unwrap_err();
        assert!(matches!(
            err,
            DecodeError::StripNotDivisible {
                width: 4,
                height: 10,
            }
        ));
    }

    #[test]
    fn decode_all_collects_per_key_errors() {
        let tmp = TempDir::new().unwrap();
        let good = tmp.path().join("good.png");
        let bad = tmp.path().join("bad.png");
        write_png(&good, 2, 2, [1, 2, 3, 255]);
        write_png(&bad, 2, 5, [1, 2, 3, 255]);
        let cfg = TexturePackConfig::default();
        let inputs = vec![
            discovered("ns:block/good", good, None),
            discovered("ns:block/bad", bad, None),
        ];
        let (ok, errs) = decode_all(&inputs, &cfg);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].key, "ns:block/good");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].0, "ns:block/bad");
    }
}
