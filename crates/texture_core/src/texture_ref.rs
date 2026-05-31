//! [`TextureRef`] — how a block names the texture it wants.
//!
//! Two forms are supported:
//!
//! - **Named** — `"namespace:block/name"` strings, resolved at runtime
//!   by whichever [`BlockAtlas`](crate::BlockAtlas) is in use.  This is
//!   the form `dd40_texture_pack` consumes when stitching Minecraft
//!   resource packs.
//! - **Direct** — a precomputed `(atlas, uv)` pair, for setups that
//!   ship a hand-built atlas and do not want runtime name lookup.

use serde::{Deserialize, Serialize};

use crate::atlas::{AtlasId, AtlasUv};

/// A pointer from a block face to a texture.
///
/// `TextureRef` is intentionally small and `Clone` — every face on
/// every renderable block carries one (when textures are configured).
///
/// # Examples
///
/// ```
/// use dd40_texture_core::TextureRef;
///
/// let by_name = TextureRef::named("minecraft:block/stone");
/// assert!(matches!(by_name, TextureRef::Named(_)));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextureRef {
    /// Looked up by string key against the active [`BlockAtlas`].
    ///
    /// Convention follows Minecraft: `"<namespace>:block/<name>"`,
    /// where the corresponding PNG lives at
    /// `assets/<namespace>/textures/block/<name>.png` inside a pack.
    Named(String),

    /// Pre-resolved UV inside a known atlas.  No runtime name lookup
    /// is required; the renderer can use the embedded
    /// [`AtlasUv`] directly.
    Direct {
        /// Which atlas to sample from.
        atlas: AtlasId,
        /// UV rect (and base array layer) inside that atlas.
        uv: AtlasUv,
    },
}

impl TextureRef {
    /// Builds a [`TextureRef::Named`] from anything convertible to a
    /// `String`.
    ///
    /// ```
    /// use dd40_texture_core::TextureRef;
    /// let r = TextureRef::named("minecraft:block/dirt");
    /// assert_eq!(r.name(), Some("minecraft:block/dirt"));
    /// ```
    pub fn named<S: Into<String>>(name: S) -> Self {
        Self::Named(name.into())
    }

    /// Returns the textual name, if this is a [`TextureRef::Named`].
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Named(s) => Some(s.as_str()),
            Self::Direct { .. } => None,
        }
    }
}
