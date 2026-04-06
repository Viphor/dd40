//! Level-of-detail (LOD) definitions and distance-based selection logic.
//!
//! LOD reduces rendering cost for distant chunks by downsampling block data
//! before meshing. Three levels are defined:
//!
//! - [`LodLevel::Lod0`] — full detail, used for nearby chunks
//! - [`LodLevel::Lod1`] — 2:1 downsampling, used for medium-distance chunks
//! - [`LodLevel::Lod2`] — 4:1 downsampling, used for far chunks
//!
//! Distance thresholds are stored in the [`LodConfig`] resource and can be
//! tuned at runtime.

use bevy::prelude::*;

/// The three supported levels of detail for chunk meshes.
///
/// Each level corresponds to a downsampling factor applied to the raw block
/// data before greedy meshing:
///
/// | Level | Downsample factor | Step size |
/// |-------|-------------------|-----------|
/// | Lod0  | 1× (none)         | 1 block   |
/// | Lod1  | 2×                | 2 blocks  |
/// | Lod2  | 4×                | 4 blocks  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodLevel {
    /// Full detail — every block is sampled. Used when the chunk is within
    /// `lod1_distance` chunk units of the player.
    Lod0,
    /// Medium detail — sample every 2nd block along each axis. Used when the
    /// chunk is between `lod1_distance` and `lod2_distance` chunk units away.
    Lod1,
    /// Low detail — sample every 4th block along each axis. Used when the
    /// chunk is farther than `lod2_distance` chunk units from the player.
    Lod2,
}

impl LodLevel {
    /// Returns the block-space step size used when iterating over the chunk
    /// for this LOD level.
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_renderer::lod::LodLevel;
    /// assert_eq!(LodLevel::Lod0.step(), 1);
    /// assert_eq!(LodLevel::Lod1.step(), 2);
    /// assert_eq!(LodLevel::Lod2.step(), 4);
    /// ```
    pub fn step(self) -> usize {
        match self {
            LodLevel::Lod0 => 1,
            LodLevel::Lod1 => 2,
            LodLevel::Lod2 => 4,
        }
    }
}

/// Bevy resource that holds the Chebyshev-distance thresholds (in chunk units)
/// controlling when each LOD level is applied.
///
/// A chunk whose Chebyshev distance from the player chunk is:
/// - `<= lod1_distance` → rendered at [`LodLevel::Lod0`]
/// - `<= lod2_distance` → rendered at [`LodLevel::Lod1`]
/// - `>  lod2_distance` → rendered at [`LodLevel::Lod2`]
///
/// # Default values
/// - `lod1_distance = 4` chunks
/// - `lod2_distance = 8` chunks
#[derive(Resource, Debug, Clone)]
pub struct LodConfig {
    /// Chebyshev distance (in chunk coordinates) at or below which a chunk
    /// is rendered at full detail ([`LodLevel::Lod0`]).
    pub lod1_distance: u32,
    /// Chebyshev distance (in chunk coordinates) at or below which a chunk
    /// is rendered at medium detail ([`LodLevel::Lod1`]).
    /// Must be `>= lod1_distance`.
    pub lod2_distance: u32,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            lod1_distance: 4,
            lod2_distance: 8,
        }
    }
}

impl LodConfig {
    /// Selects the appropriate [`LodLevel`] for a chunk at the given
    /// Chebyshev distance (in chunk units) from the player.
    ///
    /// # Arguments
    ///
    /// * `chebyshev_distance` — max(|dx|, |dz|) in chunk coordinates between
    ///   the chunk and the player's current chunk.
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_renderer::lod::{LodConfig, LodLevel};
    ///
    /// let cfg = LodConfig::default(); // lod1=4, lod2=8
    /// assert_eq!(cfg.select(0),  LodLevel::Lod0);
    /// assert_eq!(cfg.select(4),  LodLevel::Lod0);
    /// assert_eq!(cfg.select(5),  LodLevel::Lod1);
    /// assert_eq!(cfg.select(8),  LodLevel::Lod1);
    /// assert_eq!(cfg.select(9),  LodLevel::Lod2);
    /// assert_eq!(cfg.select(99), LodLevel::Lod2);
    /// ```
    pub fn select(&self, chebyshev_distance: u32) -> LodLevel {
        if chebyshev_distance <= self.lod1_distance {
            LodLevel::Lod0
        } else if chebyshev_distance <= self.lod2_distance {
            LodLevel::Lod1
        } else {
            LodLevel::Lod2
        }
    }
}

/// Computes the Chebyshev distance between two chunk positions.
///
/// Chebyshev distance is `max(|dx|, |dz|)`, which gives the minimum number of
/// "steps" needed to reach one chunk from another when diagonal movement is
/// allowed.
///
/// # Arguments
///
/// * `ax`, `az` — chunk coordinates of the first chunk
/// * `bx`, `bz` — chunk coordinates of the second chunk
pub fn chebyshev_distance(ax: i32, az: i32, bx: i32, bz: i32) -> u32 {
    let dx = (ax - bx).unsigned_abs();
    let dz = (az - bz).unsigned_abs();
    dx.max(dz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_step_sizes() {
        assert_eq!(LodLevel::Lod0.step(), 1);
        assert_eq!(LodLevel::Lod1.step(), 2);
        assert_eq!(LodLevel::Lod2.step(), 4);
    }

    #[test]
    fn lod_select_boundaries() {
        let cfg = LodConfig::default(); // lod1=4, lod2=8

        // At or inside lod1 threshold → full detail
        assert_eq!(cfg.select(0), LodLevel::Lod0);
        assert_eq!(cfg.select(1), LodLevel::Lod0);
        assert_eq!(cfg.select(4), LodLevel::Lod0);

        // Between lod1 and lod2 thresholds → medium detail
        assert_eq!(cfg.select(5), LodLevel::Lod1);
        assert_eq!(cfg.select(7), LodLevel::Lod1);
        assert_eq!(cfg.select(8), LodLevel::Lod1);

        // Beyond lod2 threshold → low detail
        assert_eq!(cfg.select(9), LodLevel::Lod2);
        assert_eq!(cfg.select(50), LodLevel::Lod2);
    }

    #[test]
    fn lod_select_custom_thresholds() {
        let cfg = LodConfig {
            lod1_distance: 2,
            lod2_distance: 5,
        };

        assert_eq!(cfg.select(0), LodLevel::Lod0);
        assert_eq!(cfg.select(2), LodLevel::Lod0);
        assert_eq!(cfg.select(3), LodLevel::Lod1);
        assert_eq!(cfg.select(5), LodLevel::Lod1);
        assert_eq!(cfg.select(6), LodLevel::Lod2);
    }

    #[test]
    fn chebyshev_distance_same_chunk() {
        assert_eq!(chebyshev_distance(3, 3, 3, 3), 0);
    }

    #[test]
    fn chebyshev_distance_axis_aligned() {
        // 5 apart in X only
        assert_eq!(chebyshev_distance(0, 0, 5, 0), 5);
        // 3 apart in Z only
        assert_eq!(chebyshev_distance(0, 0, 0, 3), 3);
    }

    #[test]
    fn chebyshev_distance_diagonal() {
        // (0,0) → (3,5): max(3,5) = 5
        assert_eq!(chebyshev_distance(0, 0, 3, 5), 5);
    }

    #[test]
    fn chebyshev_distance_negative_coords() {
        // (-2, -3) → (1, 2): dx=3, dz=5 → 5
        assert_eq!(chebyshev_distance(-2, -3, 1, 2), 5);
    }
}
