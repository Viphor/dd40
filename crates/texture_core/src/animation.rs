//! [`AnimationSpec`] — frame-table description of an animated texture.
//!
//! This mirrors Minecraft's `.mcmeta` `animation` block 1-for-1, so the
//! `dd40_texture_pack` loader can populate it without translation:
//!
//! ```json
//! { "animation": { "frametime": 4, "frames": [0, 1, 2, 3, 2, 1] } }
//! ```
//!
//! becomes
//!
//! ```ignore
//! AnimationSpec {
//!     frame_count: 4,
//!     frame_time_ms: 4 * 50, // Minecraft tick = 50 ms
//!     interpolate: false,
//!     frame_indices: vec![0, 1, 2, 3, 2, 1],
//! }
//! ```
//!
//! The loader is responsible for the tick-to-ms conversion; this type
//! stores the resolved millisecond value so the shader can use it
//! directly.

use serde::{Deserialize, Serialize};

/// Describes how an animated texture cycles through its frames.
///
/// Each animation **frame** is a separate slice of the source PNG,
/// uploaded as a successive layer in the atlas's 2D-array texture.
/// `frame_count` says how many such layers belong to this animation;
/// `frame_indices` is the playback sequence (typically `0..frame_count`
/// but may repeat or reorder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationSpec {
    /// Number of distinct frames stored in the atlas for this texture.
    ///
    /// The frames live on consecutive array layers starting at the
    /// owning [`AtlasUv`](crate::AtlasUv)'s `base_layer`.
    pub frame_count: u32,

    /// How long each step of `frame_indices` is shown, in
    /// milliseconds.
    pub frame_time_ms: u32,

    /// Whether the shader should crossfade between consecutive frames
    /// rather than snapping.  Matches Minecraft's `interpolate` field.
    pub interpolate: bool,

    /// Playback sequence, expressed as offsets into the
    /// `[base_layer .. base_layer + frame_count)` range.
    ///
    /// Must be non-empty.  The shader cycles through this slice with
    /// period `frame_indices.len() * frame_time_ms`.
    pub frame_indices: Vec<u32>,
}

impl AnimationSpec {
    /// Builds a simple animation that plays each frame once in order.
    ///
    /// Panics if `frame_count == 0`.
    pub fn linear(frame_count: u32, frame_time_ms: u32) -> Self {
        assert!(frame_count > 0, "AnimationSpec::linear requires >=1 frame");
        Self {
            frame_count,
            frame_time_ms,
            interpolate: false,
            frame_indices: (0..frame_count).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_produces_sequential_indices() {
        let a = AnimationSpec::linear(4, 50);
        assert_eq!(a.frame_indices, vec![0, 1, 2, 3]);
        assert_eq!(a.frame_count, 4);
        assert!(!a.interpolate);
    }

    #[test]
    #[should_panic]
    fn linear_rejects_zero_frames() {
        AnimationSpec::linear(0, 50);
    }
}
