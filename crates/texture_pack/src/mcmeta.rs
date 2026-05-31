//! Minecraft `.mcmeta` JSON parsing.
//!
//! Parses the subset of the `.mcmeta` format that controls animated
//! block textures, and converts it to a [`AnimationSpec`].  Other
//! fields (e.g. `villager`, `texture`, `gui`) are ignored — we accept
//! any superset and read only the keys we care about.
//!
//! # Format reference
//!
//! ```json
//! {
//!   "animation": {
//!     "frametime": 4,
//!     "interpolate": false,
//!     "frames": [0, 1, 2, { "index": 3, "time": 8 }]
//!   }
//! }
//! ```
//!
//! - `frametime` is the default tick count per frame (Minecraft tick
//!   = 50 ms).  Defaults to `1` if absent.
//! - `frames` is the playback sequence.  Each entry is either an
//!   index into the source PNG's vertical strip, or an object
//!   `{ index, time }` overriding the per-frame duration.  Defaults
//!   to `[0, 1, ..., n - 1]` for an `n`-tall strip.
//! - `interpolate` requests crossfade between frames.  Defaults to
//!   `false`.
//!
//! When per-frame `time` overrides are present, we **expand** the
//! sequence so the result still has a single `frame_time_ms`: a
//! frame with `time: 3` at base `frametime: 2` is emitted three
//! times (yielding 6 ticks total).  This keeps the GPU shader simple
//! at the cost of a slightly longer `frame_indices` array.
//!
//! # Errors
//!
//! Returns [`McmetaError`] for malformed JSON or out-of-range frame
//! indices.  An empty `frames` array is rejected.

use std::path::Path;

use dd40_texture_core::AnimationSpec;
use serde::Deserialize;

/// One Minecraft tick in milliseconds.
pub const TICK_MS: u32 = 50;

/// Errors produced by [`parse_mcmeta`].
#[derive(Debug)]
pub enum McmetaError {
    /// The file could not be read from disk.
    Io(std::io::Error),
    /// The file was not valid JSON, or did not match the expected
    /// schema.
    Json(serde_json::Error),
    /// The `frames` array referenced a frame index outside
    /// `[0, frame_count)`.
    FrameIndexOutOfRange { index: u32, frame_count: u32 },
    /// The `frames` array was present but empty.
    EmptyFrames,
}

impl std::fmt::Display for McmetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "mcmeta read error: {e}"),
            Self::Json(e) => write!(f, "mcmeta json error: {e}"),
            Self::FrameIndexOutOfRange { index, frame_count } => {
                write!(
                    f,
                    "mcmeta frame index {index} out of range for frame_count {frame_count}"
                )
            }
            Self::EmptyFrames => write!(f, "mcmeta `frames` array is empty"),
        }
    }
}

impl std::error::Error for McmetaError {}

impl From<std::io::Error> for McmetaError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for McmetaError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Reads `path` and parses it as Minecraft `.mcmeta`.
///
/// `frame_count` is the number of frames present in the source PNG
/// (derived from `height / tile_size`).  It's required so the
/// `frames` defaults can be filled in and out-of-range indices can
/// be flagged.
pub fn parse_mcmeta(path: &Path, frame_count: u32) -> Result<Option<AnimationSpec>, McmetaError> {
    let bytes = std::fs::read(path)?;
    parse_mcmeta_bytes(&bytes, frame_count)
}

/// In-memory variant of [`parse_mcmeta`].  Used by the disk reader
/// and by tests.
pub fn parse_mcmeta_bytes(
    bytes: &[u8],
    frame_count: u32,
) -> Result<Option<AnimationSpec>, McmetaError> {
    let raw: RawMcmeta = serde_json::from_slice(bytes)?;
    let Some(anim) = raw.animation else {
        return Ok(None);
    };
    let frametime = anim.frametime.unwrap_or(1).max(1);
    let interpolate = anim.interpolate.unwrap_or(false);

    let frame_indices = match anim.frames {
        None => (0..frame_count).collect::<Vec<u32>>(),
        Some(entries) => {
            if entries.is_empty() {
                return Err(McmetaError::EmptyFrames);
            }
            let mut out = Vec::with_capacity(entries.len());
            for entry in entries {
                let (index, time_ticks) = match entry {
                    RawFrame::Index(i) => (i, frametime),
                    RawFrame::Detailed { index, time } => (index, time.unwrap_or(frametime).max(1)),
                };
                if index >= frame_count {
                    return Err(McmetaError::FrameIndexOutOfRange { index, frame_count });
                }
                let repeats = time_ticks.div_ceil(frametime);
                for _ in 0..repeats {
                    out.push(index);
                }
            }
            out
        }
    };

    Ok(Some(AnimationSpec {
        frame_count,
        frame_time_ms: frametime * TICK_MS,
        interpolate,
        frame_indices,
    }))
}

#[derive(Deserialize)]
struct RawMcmeta {
    animation: Option<RawAnimation>,
}

#[derive(Deserialize)]
struct RawAnimation {
    frametime: Option<u32>,
    interpolate: Option<bool>,
    frames: Option<Vec<RawFrame>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawFrame {
    Index(u32),
    Detailed { index: u32, time: Option<u32> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_animation_block_returns_none() {
        let json = br#"{ "texture": { "blur": false } }"#;
        let parsed = parse_mcmeta_bytes(json, 1).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn defaults_to_sequential_indices_when_frames_absent() {
        let json = br#"{ "animation": { "frametime": 4 } }"#;
        let a = parse_mcmeta_bytes(json, 3).unwrap().unwrap();
        assert_eq!(a.frame_indices, vec![0, 1, 2]);
        assert_eq!(a.frame_time_ms, 4 * TICK_MS);
        assert_eq!(a.frame_count, 3);
        assert!(!a.interpolate);
    }

    #[test]
    fn parses_simple_index_list() {
        let json = br#"{ "animation": { "frames": [0, 1, 2, 1] } }"#;
        let a = parse_mcmeta_bytes(json, 3).unwrap().unwrap();
        assert_eq!(a.frame_indices, vec![0, 1, 2, 1]);
        assert_eq!(a.frame_time_ms, TICK_MS);
    }

    #[test]
    fn detailed_frame_time_expands_repeats() {
        let json =
            br#"{ "animation": { "frametime": 2, "frames": [0, { "index": 1, "time": 6 }] } }"#;
        let a = parse_mcmeta_bytes(json, 2).unwrap().unwrap();
        // frametime 2 base, frame 1 wants 6 ticks → 3 repeats.
        assert_eq!(a.frame_indices, vec![0, 1, 1, 1]);
    }

    #[test]
    fn interpolate_flag_round_trips() {
        let json = br#"{ "animation": { "interpolate": true, "frames": [0] } }"#;
        let a = parse_mcmeta_bytes(json, 1).unwrap().unwrap();
        assert!(a.interpolate);
    }

    #[test]
    fn out_of_range_index_is_rejected() {
        let json = br#"{ "animation": { "frames": [0, 5] } }"#;
        let err = parse_mcmeta_bytes(json, 2).unwrap_err();
        assert!(matches!(
            err,
            McmetaError::FrameIndexOutOfRange {
                index: 5,
                frame_count: 2
            }
        ));
    }

    #[test]
    fn empty_frames_is_rejected() {
        let json = br#"{ "animation": { "frames": [] } }"#;
        let err = parse_mcmeta_bytes(json, 1).unwrap_err();
        assert!(matches!(err, McmetaError::EmptyFrames));
    }

    #[test]
    fn unknown_top_level_keys_are_ignored() {
        let json = br#"{ "villager": { "hat": "partial" }, "animation": { "frames": [0] } }"#;
        let a = parse_mcmeta_bytes(json, 1).unwrap().unwrap();
        assert_eq!(a.frame_indices, vec![0]);
    }
}
