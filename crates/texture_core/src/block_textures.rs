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
    /// Optional overlay textures, one per face.  When present, the
    /// renderer alpha-composites the overlay on top of the base and
    /// multiplies the overlay's RGB by the per-block colour — matching
    /// Minecraft's grass-side / grass-top behaviour where a separate
    /// greyscale overlay carries the biome tint.
    ///
    /// All six fields default to `None` and are `#[serde(default)]` so
    /// older serialised data deserialises with no overlays.
    #[serde(default)]
    pub top_overlay: Option<TextureRef>,
    #[serde(default)]
    pub bottom_overlay: Option<TextureRef>,
    #[serde(default)]
    pub north_overlay: Option<TextureRef>,
    #[serde(default)]
    pub south_overlay: Option<TextureRef>,
    #[serde(default)]
    pub east_overlay: Option<TextureRef>,
    #[serde(default)]
    pub west_overlay: Option<TextureRef>,
    /// Whether the renderer should multiply the per-block colour
    /// (`BlockDefinition::color`) into the sampled texel.
    ///
    /// `false` (default): the texture is shown exactly as authored,
    /// matching how stone, dirt, sand, etc. behave in Minecraft.
    /// `true`: the texture is tinted by the block colour — used in
    /// Minecraft for leaves and water where the underlying greyscale
    /// texture is multiplied by a biome-driven RGB tint.  Note that
    /// `tinted` and overlays are independent — grass uses overlays
    /// (and `tinted = false`) while leaves use `tinted = true` with
    /// no overlay.
    ///
    /// `#[serde(default)]` so older serialised data without this field
    /// deserialises with `tinted = false`.
    #[serde(default)]
    pub tinted: bool,
    /// Per-face overrides for [`Self::tinted`].  `Some(b)` overrides
    /// the global flag for that face; `None` (default) falls back to
    /// the global flag.
    ///
    /// Used by Minecraft's grass block, whose top face is a tinted
    /// greyscale, sides use an overlay (so the base must NOT be
    /// tinted), and bottom is plain dirt (also not tinted).
    #[serde(default)]
    pub top_tinted: Option<bool>,
    #[serde(default)]
    pub bottom_tinted: Option<bool>,
    #[serde(default)]
    pub north_tinted: Option<bool>,
    #[serde(default)]
    pub south_tinted: Option<bool>,
    #[serde(default)]
    pub east_tinted: Option<bool>,
    #[serde(default)]
    pub west_tinted: Option<bool>,
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
            ..Self::default()
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
            ..Self::default()
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
            ..Self::default()
        }
    }

    /// Looks up the base texture for a specific face.
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

    /// Looks up the overlay texture for a specific face, if one is
    /// configured.  The overlay is composited on top of the base by
    /// the renderer and its RGB is multiplied by the per-block colour.
    pub fn overlay(&self, face: Face) -> Option<&TextureRef> {
        match face {
            Face::Top => self.top_overlay.as_ref(),
            Face::Bottom => self.bottom_overlay.as_ref(),
            Face::North => self.north_overlay.as_ref(),
            Face::South => self.south_overlay.as_ref(),
            Face::East => self.east_overlay.as_ref(),
            Face::West => self.west_overlay.as_ref(),
        }
    }

    /// Whether the renderer should whole-output-tint this specific
    /// face.  Falls back to [`Self::tinted`] when no per-face override
    /// has been set.
    pub fn tinted_for(&self, face: Face) -> bool {
        let override_ = match face {
            Face::Top => self.top_tinted,
            Face::Bottom => self.bottom_tinted,
            Face::North => self.north_tinted,
            Face::South => self.south_tinted,
            Face::East => self.east_tinted,
            Face::West => self.west_tinted,
        };
        override_.unwrap_or(self.tinted)
    }

    /// Sets the per-face tint override for a single face.  `Some(b)`
    /// overrides the global flag; `None` clears the override.
    pub fn with_tint_for(mut self, face: Face, tinted: Option<bool>) -> Self {
        match face {
            Face::Top => self.top_tinted = tinted,
            Face::Bottom => self.bottom_tinted = tinted,
            Face::North => self.north_tinted = tinted,
            Face::South => self.south_tinted = tinted,
            Face::East => self.east_tinted = tinted,
            Face::West => self.west_tinted = tinted,
        }
        self
    }

    /// Returns `true` if every face has a texture assigned.
    pub fn is_complete(&self) -> bool {
        Face::ALL.iter().all(|&f| self.get(f).is_some())
    }

    /// Returns `true` if no face has a texture assigned.
    pub fn is_empty(&self) -> bool {
        Face::ALL.iter().all(|&f| self.get(f).is_none())
    }

    /// Builder-style setter for [`Self::tinted`].  Returns `self` so
    /// it chains naturally with the other constructors:
    ///
    /// ```
    /// # use dd40_texture_core::{BlockTextures, TextureRef};
    /// let leaves = BlockTextures::all(TextureRef::named("minecraft:block/oak_leaves"))
    ///     .with_tint(true);
    /// assert!(leaves.tinted);
    /// ```
    #[must_use]
    pub fn with_tint(mut self, tinted: bool) -> Self {
        self.tinted = tinted;
        self
    }

    /// Assigns the same overlay texture to all four side faces (the
    /// common "grass-side" pattern).
    ///
    /// ```
    /// # use dd40_texture_core::{BlockTextures, Face, TextureRef};
    /// let grass = BlockTextures::top_bottom_sides(
    ///     TextureRef::named("minecraft:block/grass_block_top"),
    ///     TextureRef::named("minecraft:block/dirt"),
    ///     TextureRef::named("minecraft:block/grass_block_side"),
    /// )
    /// .with_side_overlay(TextureRef::named(
    ///     "minecraft:block/grass_block_side_overlay",
    /// ));
    /// assert!(grass.overlay(Face::North).is_some());
    /// assert!(grass.overlay(Face::Top).is_none());
    /// ```
    #[must_use]
    pub fn with_side_overlay(mut self, t: TextureRef) -> Self {
        self.north_overlay = Some(t.clone());
        self.south_overlay = Some(t.clone());
        self.east_overlay = Some(t.clone());
        self.west_overlay = Some(t);
        self
    }

    /// Assigns the same overlay texture to every face.
    #[must_use]
    pub fn with_overlay_all(mut self, t: TextureRef) -> Self {
        self.top_overlay = Some(t.clone());
        self.bottom_overlay = Some(t.clone());
        self.north_overlay = Some(t.clone());
        self.south_overlay = Some(t.clone());
        self.east_overlay = Some(t.clone());
        self.west_overlay = Some(t);
        self
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

    #[test]
    fn with_side_overlay_assigns_only_to_side_faces() {
        let t = BlockTextures::top_bottom_sides(
            TextureRef::named("top"),
            TextureRef::named("bot"),
            TextureRef::named("side"),
        )
        .with_side_overlay(TextureRef::named("side_overlay"));
        for f in [Face::North, Face::South, Face::East, Face::West] {
            assert_eq!(t.overlay(f), Some(&TextureRef::named("side_overlay")));
        }
        assert_eq!(t.overlay(Face::Top), None);
        assert_eq!(t.overlay(Face::Bottom), None);
    }

    #[test]
    fn default_has_no_overlays() {
        let t = BlockTextures::all(TextureRef::named("x"));
        for f in Face::ALL {
            assert!(
                t.overlay(f).is_none(),
                "face {f:?} unexpectedly has overlay"
            );
        }
    }
}
