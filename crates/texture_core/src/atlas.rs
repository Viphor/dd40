//! [`BlockAtlas`] — runtime lookup table populated by an atlas owner.
//!
//! The atlas itself (the GPU texture) is owned by whichever plugin
//! built it.  This module defines only the **lookup contract** so the
//! renderer, the inventory icon cache, and any other texture consumer
//! can find a texture without knowing how it was loaded.
//!
//! Multiple atlases may coexist (each identified by an [`AtlasId`]) —
//! for example a default Minecraft-pack atlas plus a separate atlas
//! produced by a mod.  Consumers select an atlas through the
//! [`TextureRef::Direct`](crate::TextureRef::Direct) form, or rely on
//! the implementation's name resolution to pick one for
//! [`TextureRef::Named`](crate::TextureRef::Named).

use bevy::asset::Handle;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::SystemSet;
use bevy::image::Image;
use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::animation::AnimationSpec;
use crate::render_layer::RenderLayer;
use crate::texture_ref::TextureRef;

/// Opaque identifier for an atlas.
///
/// Atlases are produced by Tier-1 plugins (e.g. `dd40_texture_pack`).
/// Each registered atlas gets a unique `AtlasId`; consumers carry it
/// around when they need to disambiguate between several atlases
/// loaded simultaneously.
///
/// `AtlasId(0)` is reserved for the default / primary atlas — most
/// single-pack setups never need to think about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct AtlasId(pub u32);

/// UV rectangle within an atlas, plus the starting array layer.
///
/// `min` and `max` are normalised to `[0, 1]` within the source layer.
/// `base_layer` is the first array layer of the texture; for static
/// textures the slot occupies only that layer, for animated textures
/// the frames occupy `[base_layer, base_layer + frame_count)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AtlasUv {
    /// Top-left UV (inclusive), in `[0, 1]`.
    pub min: Vec2,
    /// Bottom-right UV (exclusive), in `[0, 1]`.
    pub max: Vec2,
    /// First array-texture layer holding the texel data.
    pub base_layer: u32,
}

impl AtlasUv {
    /// Builds a UV rect spanning the entire layer.
    pub fn full_layer(layer: u32) -> Self {
        Self {
            min: Vec2::ZERO,
            max: Vec2::ONE,
            base_layer: layer,
        }
    }
}

/// Fully-resolved description of a texture inside a known atlas.
///
/// Returned by [`BlockAtlas::resolve`].  Everything the renderer needs
/// to actually draw the texture lives here — no further lookups
/// required.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTexture {
    /// Which atlas to sample from.
    pub atlas: AtlasId,
    /// UV rect + base layer.
    pub uv: AtlasUv,
    /// Final render layer.  Either the texture's auto-classified
    /// layer, or a per-block override.
    pub render_layer: RenderLayer,
    /// Animation spec, if this is an animated texture.  When `None`,
    /// the texture is static and `uv.base_layer` is the only layer
    /// used.
    pub animation: Option<AnimationSpec>,
}

/// Runtime lookup resource: turns a [`TextureRef`] into a
/// [`ResolvedTexture`].
///
/// The texture-pack loader inserts an implementation of this trait as
/// a Bevy [`Resource`] once the atlas is built.  A no-op default impl
/// is provided so the renderer can query [`BlockAtlas`] safely before
/// any pack is loaded (it will see `None` for every lookup and fall
/// back to the colour path).
#[derive(Resource, Debug, Default, Clone)]
pub struct BlockAtlas {
    /// Boxed lookup implementation.  `None` means no atlas is
    /// installed yet; every `resolve` call returns `None`.
    inner: Option<std::sync::Arc<dyn BlockAtlasSource>>,
}

impl BlockAtlas {
    /// Installs a backing source.  Called by the atlas-owning plugin
    /// once its atlas is ready.
    pub fn set_source(&mut self, source: std::sync::Arc<dyn BlockAtlasSource>) {
        self.inner = Some(source);
    }

    /// Clears the installed source.  Used when reloading a pack.
    pub fn clear(&mut self) {
        self.inner = None;
    }

    /// Resolves a [`TextureRef`] against the installed source.
    ///
    /// Returns `None` if no source is installed or if the source has
    /// no entry for the given reference.  Callers should treat this
    /// as "fall back to the per-block colour".
    pub fn resolve(&self, r: &TextureRef) -> Option<ResolvedTexture> {
        match (self.inner.as_ref(), r) {
            (Some(src), r) => src.resolve(r),
            (None, TextureRef::Direct { atlas, uv }) => Some(ResolvedTexture {
                atlas: *atlas,
                uv: *uv,
                render_layer: RenderLayer::Opaque,
                animation: None,
            }),
            (None, TextureRef::Named(_)) => None,
        }
    }

    /// Returns the handle for the named atlas's array texture, if the
    /// installed source has one and recognises the id.
    ///
    /// The renderer uses this to bind the atlas texture into its
    /// material.
    pub fn texture(&self, atlas: AtlasId) -> Option<Handle<Image>> {
        self.inner.as_ref().and_then(|s| s.texture(atlas))
    }

    /// Returns `true` if an atlas source has been installed.
    pub fn is_ready(&self) -> bool {
        self.inner.is_some()
    }
}

/// Backing trait implemented by atlas owners.
///
/// Implementations must be `Send + Sync` so the [`BlockAtlas`] resource
/// can be shared across systems.
pub trait BlockAtlasSource: Send + Sync + std::fmt::Debug {
    /// Resolves a [`TextureRef`] to a fully-described atlas entry, or
    /// `None` if the reference is unknown.
    fn resolve(&self, r: &TextureRef) -> Option<ResolvedTexture>;

    /// Returns the GPU texture handle for an atlas this source owns,
    /// or `None` if the id is unknown.
    fn texture(&self, atlas: AtlasId) -> Option<Handle<Image>>;
}

/// System set anchor: "atlas loading is complete; meshing may begin".
///
/// The texture-pack loader places its atlas-build system in this set
/// so consumers (renderer, inventory icons) can order themselves with
/// `.after(AtlasReady)`.  Systems that only *read* the
/// [`BlockAtlas`] resource at runtime do not need to do this — the
/// fallback path handles "not yet ready" gracefully.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasReady;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_layer_spans_unit_uv() {
        let uv = AtlasUv::full_layer(3);
        assert_eq!(uv.min, Vec2::ZERO);
        assert_eq!(uv.max, Vec2::ONE);
        assert_eq!(uv.base_layer, 3);
    }

    #[test]
    fn default_block_atlas_has_no_source_and_falls_back_for_named() {
        let atlas = BlockAtlas::default();
        assert!(!atlas.is_ready());
        assert!(atlas.resolve(&TextureRef::named("x:y/z")).is_none());
    }

    #[test]
    fn default_block_atlas_passes_through_direct_refs() {
        let atlas = BlockAtlas::default();
        let uv = AtlasUv::full_layer(7);
        let resolved = atlas
            .resolve(&TextureRef::Direct {
                atlas: AtlasId(2),
                uv,
            })
            .expect("direct ref must resolve even without an installed source");
        assert_eq!(resolved.atlas, AtlasId(2));
        assert_eq!(resolved.uv.base_layer, 7);
        assert_eq!(resolved.render_layer, RenderLayer::Opaque);
        assert!(resolved.animation.is_none());
    }

    #[derive(Debug)]
    struct OneEntry {
        key: &'static str,
        out: ResolvedTexture,
    }

    impl BlockAtlasSource for OneEntry {
        fn resolve(&self, r: &TextureRef) -> Option<ResolvedTexture> {
            match r {
                TextureRef::Named(s) if s == self.key => Some(self.out.clone()),
                _ => None,
            }
        }
        fn texture(&self, _atlas: AtlasId) -> Option<Handle<Image>> {
            None
        }
    }

    #[test]
    fn installed_source_resolves_named_refs() {
        let mut atlas = BlockAtlas::default();
        atlas.set_source(std::sync::Arc::new(OneEntry {
            key: "ns:block/foo",
            out: ResolvedTexture {
                atlas: AtlasId(0),
                uv: AtlasUv::full_layer(42),
                render_layer: RenderLayer::Cutout,
                animation: None,
            },
        }));
        let resolved = atlas.resolve(&TextureRef::named("ns:block/foo")).unwrap();
        assert_eq!(resolved.uv.base_layer, 42);
        assert_eq!(resolved.render_layer, RenderLayer::Cutout);
        assert!(atlas.resolve(&TextureRef::named("nope")).is_none());
    }
}
