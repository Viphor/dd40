use bevy::prelude::*;

/// Ordering anchor for render-frame visual systems.
///
/// Both `dd40_network` and `dd40_player` import this set to enforce a
/// deterministic order between frame interpolation and camera-follow without a
/// direct crate dependency on each other.
///
/// **Expected order (both in `Update`):**
/// 1. [`CharacterRenderSet::FrameInterpolation`] — write the smoothed `Transform`
/// 2. [`CharacterRenderSet::CameraSync`] — follow the now-smoothed `Transform`
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharacterRenderSet {
    /// Write the visual `Transform` for predicted characters.
    ///
    /// Frame-interpolation and visual-correction application belong here.
    FrameInterpolation,
    /// Sync the camera (or any other follower) to the player `Transform`.
    ///
    /// Always runs **after** [`CharacterRenderSet::FrameInterpolation`].
    CameraSync,
}
