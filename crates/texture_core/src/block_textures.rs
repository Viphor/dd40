//! [`BlockTextures`] — per-face texture assignment for a block.
//!
//! Each cube has six faces.  `BlockTextures` carries one optional
//! [`TextureRef`] per face; missing faces fall back to the block's
//! colour at render time.
//!
//! [`BlockTextures`] is a [`BlockData`], attached via
//! [`BlockDefinition::with_data`](dd40_core::block::BlockDefinition::with_data),
//! and registered with the [`BlockDataTypeRegistry`] by
//! [`TextureCorePlugin`](crate::TextureCorePlugin).
//!
//! [`BlockData`]: dd40_core::block::BlockData
//! [`BlockDataTypeRegistry`]: dd40_core::block::BlockDataTypeRegistry

use std::any::Any;

use dd40_core::block::BlockData;
use serde::{Deserialize, Serialize};

use crate::texture_ref::TextureRef;

/// The six cube faces, in a stable serialisation order.
///
/// Axis convention matches the rest of the renderer:
/// `+X = East`, `-X = West`, `+Y = Top`, `-Y = Bottom`,
/// `+Z = South`, `-Z = North`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Face {
    /// `+Y`
    Top,
    /// `-Y`
    Bottom,
    /// `-Z`
    North,
    /// `+Z`
    South,
    /// `+X`
    East,
    /// `-X`
    West,
}

impl Face {
    /// All six faces, in the order [`Face`] is declared.  Useful for
    /// iteration without hard-coding the list.
    pub const ALL: [Face; 6] = [
        Face::Top,
        Face::Bottom,
        Face::North,
        Face::South,
        Face::East,
        Face::West,
    ];
}

/// Per-face texture assignment for a block.
///
/// Construct via one of the builders below — direct field access is
/// also supported but the builders are easier to read at call sites.
///
/// # Examples
///
/// ```
/// use dd40_texture_core::{BlockTextures, Face, TextureRef};
///
/// // Every face uses the same texture.
/// let t = BlockTextures::all(TextureRef::named("ns:block/stone"));
/// for f in Face::ALL {
///     assert!(t.get(f).is_some());
/// }
///
/// // Log-style block: top + bottom share one texture, the four sides
/// // share another.
/// let log = BlockTextures::top_bottom_sides(
///     TextureRef::named("ns:block/oak_log_top"),
///     TextureRef::named("ns:block/oak_log_top"),
///     TextureRef::named("ns:block/oak_log"),
/// );
/// assert_eq!(log.get(Face::Top), log.get(Face::Bottom));
/// assert_eq!(log.get(Face::North), log.get(Face::West));
/// assert_ne!(log.get(Face::Top), log.get(Face::North));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockTextures {
    /// `+Y`
    pub top: Option<TextureRef>,
    /// `-Y`
    pub bottom: Option<TextureRef>,
    /// `-Z`
    pub north: Option<TextureRef>,
    /// `+Z`
    pub south: Option<TextureRef>,
    /// `+X`
    pub east: Option<TextureRef>,
    /// `-X`
    pub west: Option<TextureRef>,
}

impl BlockTextures {
    /// Empty texture set.  Every face is `None`; renderer falls back to
    /// the per-block colour.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Assigns the same texture to all six faces.
    pub fn all(t: TextureRef) -> Self {
        Self {
            top: Some(t.clone()),
            bottom: Some(t.clone()),
            north: Some(t.clone()),
            south: Some(t.clone()),
            east: Some(t.clone()),
            west: Some(t),
        }
    }

    /// Common "log / grass" pattern: separate top, bottom, and a
    /// single side texture used by all four side faces.
    pub fn top_bottom_sides(top: TextureRef, bottom: TextureRef, sides: TextureRef) -> Self {
        Self {
            top: Some(top),
            bottom: Some(bottom),
            north: Some(sides.clone()),
            south: Some(sides.clone()),
            east: Some(sides.clone()),
            west: Some(sides),
        }
    }

    /// Full per-face control.  Pass `None` for faces that should fall
    /// back to colour.
    pub fn per_face(
        top: Option<TextureRef>,
        bottom: Option<TextureRef>,
        north: Option<TextureRef>,
        south: Option<TextureRef>,
        east: Option<TextureRef>,
        west: Option<TextureRef>,
    ) -> Self {
        Self {
            top,
            bottom,
            north,
            south,
            east,
            west,
        }
    }

    /// Looks up the texture for a specific face.
    pub fn get(&self, face: Face) -> Option<&TextureRef> {
        match face {
            Face::Top => self.top.as_ref(),
            Face::Bottom => self.bottom.as_ref(),
            Face::North => self.north.as_ref(),
            Face::South => self.south.as_ref(),
            Face::East => self.east.as_ref(),
            Face::West => self.west.as_ref(),
        }
    }

    /// Returns `true` if every face has a texture assigned.
    pub fn is_complete(&self) -> bool {
        Face::ALL.iter().all(|&f| self.get(f).is_some())
    }

    /// Returns `true` if no face has a texture assigned.
    pub fn is_empty(&self) -> bool {
        Face::ALL.iter().all(|&f| self.get(f).is_none())
    }
}

impl BlockData for BlockTextures {
    fn type_key(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn clone_box(&self) -> Box<dyn BlockData> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_assigns_every_face() {
        let t = BlockTextures::all(TextureRef::named("x"));
        assert!(t.is_complete());
        assert!(!t.is_empty());
        for f in Face::ALL {
            assert_eq!(t.get(f), Some(&TextureRef::named("x")));
        }
    }

    #[test]
    fn empty_is_empty() {
        let t = BlockTextures::empty();
        assert!(t.is_empty());
        assert!(!t.is_complete());
    }

    #[test]
    fn top_bottom_sides_groups_correctly() {
        let t = BlockTextures::top_bottom_sides(
            TextureRef::named("top"),
            TextureRef::named("bot"),
            TextureRef::named("side"),
        );
        assert_eq!(t.get(Face::Top), Some(&TextureRef::named("top")));
        assert_eq!(t.get(Face::Bottom), Some(&TextureRef::named("bot")));
        for f in [Face::North, Face::South, Face::East, Face::West] {
            assert_eq!(t.get(f), Some(&TextureRef::named("side")));
        }
    }

    #[test]
    fn per_face_respects_none() {
        let t = BlockTextures::per_face(
            Some(TextureRef::named("only-top")),
            None,
            None,
            None,
            None,
            None,
        );
        assert!(t.get(Face::Top).is_some());
        for f in [
            Face::Bottom,
            Face::North,
            Face::South,
            Face::East,
            Face::West,
        ] {
            assert!(t.get(f).is_none());
        }
    }

    #[test]
    fn block_data_clone_box_round_trips() {
        let t = BlockTextures::all(TextureRef::named("ns:block/stone"));
        let boxed = BlockData::clone_box(&t);
        let back = boxed.as_any().downcast_ref::<BlockTextures>().cloned();
        assert_eq!(back, Some(t));
    }
}
