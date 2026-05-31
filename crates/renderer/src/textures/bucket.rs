//! Per-face merge buckets for textured meshing.
//!
//! Greedy meshing today merges adjacent faces if they have the same
//! [`BlockId`].  With textures that is no longer sufficient: two
//! adjacent faces with the same `BlockId` but **different texture
//! references** (e.g. the top of a log vs its side) must not merge into
//! a single rectangle because they end up in different atlas layers
//! and would have to be split anyway when building the per-bucket
//! mesh.
//!
//! A *bucket* is the equivalence class faces fall into for both
//! greedy-merging and mesh-output purposes.  Two faces merge only if
//! their `(BlockId, BucketKey)` is identical.  One Bevy mesh entity is
//! emitted per `(RenderLayer, BucketKey)` per chunk, each paired with
//! a [`BlockAtlasMaterial`](super::material::BlockAtlasMaterial)
//! configured for the bucket's atlas layer.
//!
//! ## Resolution order
//!
//! For each block + face:
//!
//! 1. If the block has no [`BlockTextures`] attached, or the atlas is
//!    not ready, or the texture name is unknown → [`BucketKey::Untextured`].
//!    These faces fall through to the renderer's existing colour-only
//!    path.
//! 2. If the texture resolves and is static → [`BucketKey::Static`].
//! 3. Animated textures are deferred to a later commit.  Until then
//!    they are treated as [`BucketKey::Untextured`] so they at least
//!    render, just without the per-frame layer cycling.

use std::collections::HashMap;

use dd40_core::block::{BlockId, BlockRegistry};
use dd40_texture_core::{
    AtlasId, AtlasUv, BlockAtlas, BlockTextures, Face, RenderLayer, ResolvedTexture,
};

#[cfg(test)]
use dd40_texture_core::TextureRef;

use crate::face_culling::FaceDir;

/// Equivalence class a single block face falls into for mesh splitting.
///
/// One Bevy mesh entity (and one
/// [`BlockAtlasMaterial`](super::material::BlockAtlasMaterial)
/// instance) is emitted per `BucketKey` per chunk.  Greedy meshing
/// itself keys by `BlockId` alone — within a single face-direction
/// slice every cell with the same `BlockId` necessarily resolves to
/// the same texture, so no extra merge key is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BucketKey {
    /// Block has no texture, or the atlas is not ready, or the
    /// texture lookup failed.  Rendered through the colour-only
    /// fallback path using `BlockDefinition::color`.
    Untextured,
    /// Block face samples a static (non-animated) atlas layer.
    Static {
        /// Which atlas to sample from.
        atlas_id: AtlasId,
        /// Array layer to sample.  Same on every face vertex in the
        /// bucket.
        atlas_layer: u32,
        /// Composition pass this bucket renders in.
        render_layer: RenderLayer,
    },
}

/// Resolved per-face texture data: which bucket the face belongs to
/// and (for textured faces) where in its atlas layer the texels live.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceTextureInfo {
    /// Mesh / material grouping.
    pub bucket: BucketKey,
    /// UV sub-rectangle within the atlas layer; `None` for
    /// [`BucketKey::Untextured`].  When `Some`, the mesh builder
    /// remaps the unit-square per-quad UV (`(0,0)..(1,1)`) into this
    /// rect so a single mesh + single material can carry many tiles.
    pub uv: Option<AtlasUv>,
}

impl FaceTextureInfo {
    /// Untextured fallback: belongs in the colour-only bucket, has no
    /// atlas UV.
    pub const UNTEXTURED: Self = Self {
        bucket: BucketKey::Untextured,
        uv: None,
    };
}

/// Per-block face-texture lookup: one entry per face, indexed by the
/// declaration order in [`Face::ALL`].
///
/// Use [`face_dir_to_face`] + [`face_index`] to project a renderer
/// [`FaceDir`] into this array.
pub type FaceBuckets = [FaceTextureInfo; 6];

/// Maps a renderer [`FaceDir`] to the corresponding
/// [`dd40_texture_core::Face`].
///
/// The renderer's `FaceDir` is purely geometric (`PosX` / `NegX` / …),
/// while texture_core's `Face` is the named convention
/// (`Top` / `Bottom` / `North` / …).  Both use the same axis convention
/// (`+Y = Top`, `+Z = South`, `+X = East`) so this is a direct mapping.
pub fn face_dir_to_face(dir: FaceDir) -> Face {
    match dir {
        FaceDir::PosX => Face::East,
        FaceDir::NegX => Face::West,
        FaceDir::PosY => Face::Top,
        FaceDir::NegY => Face::Bottom,
        FaceDir::PosZ => Face::South,
        FaceDir::NegZ => Face::North,
    }
}

/// Index into [`FaceBuckets`] for a given face.  Keeps callers free of
/// the [`Face`] enum's internal ordering.
pub fn face_index(face: Face) -> usize {
    Face::ALL
        .iter()
        .position(|f| *f == face)
        .expect("Face::ALL must contain every Face variant — guaranteed by texture_core")
}

/// Resolves a single face to a [`FaceTextureInfo`].
///
/// `block_textures` may be `None` if the block has no [`BlockTextures`]
/// attached (which is fine — the face falls into the untextured bucket).
pub fn resolve_face_bucket(
    block_textures: Option<&BlockTextures>,
    face: Face,
    atlas: &BlockAtlas,
) -> FaceTextureInfo {
    let Some(textures) = block_textures else {
        return FaceTextureInfo::UNTEXTURED;
    };
    let Some(texture_ref) = textures.get(face) else {
        return FaceTextureInfo::UNTEXTURED;
    };
    let Some(resolved) = atlas.resolve(texture_ref) else {
        return FaceTextureInfo::UNTEXTURED;
    };
    info_from_resolved(&resolved)
}

/// Builds a [`FaceTextureInfo`] from a fully-resolved atlas entry.
///
/// Animated textures currently fall back to [`BucketKey::Untextured`];
/// the animated-bucket variant lands in a follow-up commit.
fn info_from_resolved(r: &ResolvedTexture) -> FaceTextureInfo {
    if r.animation.is_some() {
        return FaceTextureInfo::UNTEXTURED;
    }
    FaceTextureInfo {
        bucket: BucketKey::Static {
            atlas_id: r.atlas,
            atlas_layer: r.uv.base_layer,
            render_layer: r.render_layer,
        },
        uv: Some(r.uv),
    }
}

/// Precomputes the [`FaceBuckets`] for every registered block.
///
/// Called once per atlas-ready frame inside the mesh-task dispatcher;
/// the result is cloned into each spawned task so the task does not
/// need to touch the [`BlockRegistry`] or [`BlockAtlas`] resources
/// directly.
///
/// Blocks that have no [`BlockTextures`] receive
/// `[FaceTextureInfo::UNTEXTURED; 6]`.
pub fn compute_face_buckets(
    registry: &BlockRegistry,
    atlas: &BlockAtlas,
) -> HashMap<BlockId, FaceBuckets> {
    let mut out = HashMap::new();
    for def in registry.iter() {
        let textures = def.data::<BlockTextures>();
        let mut buckets = [FaceTextureInfo::UNTEXTURED; 6];
        for (i, face) in Face::ALL.iter().enumerate() {
            buckets[i] = resolve_face_bucket(textures, *face, atlas);
        }
        out.insert(def.id, buckets);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use dd40_texture_core::{AtlasUv, BlockAtlasSource};
    use std::sync::Arc;

    #[derive(Debug)]
    struct StubAtlas {
        // Map texture name → (layer, render_layer).
        entries: HashMap<String, (u32, RenderLayer)>,
    }

    impl BlockAtlasSource for StubAtlas {
        fn resolve(&self, r: &TextureRef) -> Option<ResolvedTexture> {
            let TextureRef::Named(name) = r else {
                return None;
            };
            let (layer, rl) = self.entries.get(name).copied()?;
            Some(ResolvedTexture {
                atlas: AtlasId(0),
                uv: AtlasUv {
                    min: Vec2::ZERO,
                    max: Vec2::ONE,
                    base_layer: layer,
                },
                render_layer: rl,
                animation: None,
            })
        }
        fn texture(&self, _atlas: AtlasId) -> Option<bevy::asset::Handle<bevy::image::Image>> {
            None
        }
    }

    fn stub_atlas(entries: &[(&str, u32, RenderLayer)]) -> BlockAtlas {
        let map = entries
            .iter()
            .map(|(n, l, r)| ((*n).to_string(), (*l, *r)))
            .collect();
        let mut a = BlockAtlas::default();
        a.set_source(Arc::new(StubAtlas { entries: map }));
        a
    }

    #[test]
    fn untextured_when_no_block_textures_attached() {
        let atlas = stub_atlas(&[("ns:foo", 0, RenderLayer::Opaque)]);
        let b = resolve_face_bucket(None, Face::Top, &atlas);
        assert_eq!(b.bucket, BucketKey::Untextured);
        assert!(b.uv.is_none());
    }

    #[test]
    fn untextured_when_face_has_no_texture_ref() {
        let atlas = stub_atlas(&[]);
        let textures = BlockTextures::default();
        let b = resolve_face_bucket(Some(&textures), Face::Top, &atlas);
        assert_eq!(b.bucket, BucketKey::Untextured);
        assert!(b.uv.is_none());
    }

    #[test]
    fn untextured_when_atlas_does_not_know_texture() {
        let atlas = stub_atlas(&[]);
        let textures = BlockTextures::all(TextureRef::named("ns:missing"));
        let b = resolve_face_bucket(Some(&textures), Face::Top, &atlas);
        assert_eq!(b.bucket, BucketKey::Untextured);
    }

    #[test]
    fn static_when_texture_resolves() {
        let atlas = stub_atlas(&[("ns:stone", 7, RenderLayer::Opaque)]);
        let textures = BlockTextures::all(TextureRef::named("ns:stone"));
        let b = resolve_face_bucket(Some(&textures), Face::Top, &atlas);
        assert_eq!(
            b.bucket,
            BucketKey::Static {
                atlas_id: AtlasId(0),
                atlas_layer: 7,
                render_layer: RenderLayer::Opaque,
            }
        );
        assert!(b.uv.is_some());
    }

    #[test]
    fn per_face_textures_produce_different_buckets() {
        let atlas = stub_atlas(&[
            ("ns:grass_top", 0, RenderLayer::Opaque),
            ("ns:grass_side", 1, RenderLayer::Opaque),
            ("ns:dirt", 2, RenderLayer::Opaque),
        ]);
        let textures = BlockTextures::top_bottom_sides(
            TextureRef::named("ns:grass_top"),
            TextureRef::named("ns:dirt"),
            TextureRef::named("ns:grass_side"),
        );
        let top = resolve_face_bucket(Some(&textures), Face::Top, &atlas).bucket;
        let bot = resolve_face_bucket(Some(&textures), Face::Bottom, &atlas).bucket;
        let side = resolve_face_bucket(Some(&textures), Face::North, &atlas).bucket;
        assert_ne!(top, bot);
        assert_ne!(top, side);
        assert_ne!(bot, side);
    }

    #[test]
    fn animated_falls_back_to_untextured_for_now() {
        let r = ResolvedTexture {
            atlas: AtlasId(0),
            uv: AtlasUv {
                min: Vec2::ZERO,
                max: Vec2::ONE,
                base_layer: 4,
            },
            render_layer: RenderLayer::Translucent,
            animation: Some(dd40_texture_core::AnimationSpec {
                frame_count: 4,
                frame_time_ms: 100,
                interpolate: false,
                frame_indices: vec![0, 1, 2, 3],
            }),
        };
        assert_eq!(info_from_resolved(&r).bucket, BucketKey::Untextured);
    }

    #[test]
    fn face_dir_to_face_round_trips_for_all_six() {
        let faces: Vec<Face> = FaceDir::ALL.iter().copied().map(face_dir_to_face).collect();
        let mut sorted = faces.clone();
        sorted.sort_by_key(|f| face_index(*f));
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "expected six distinct face mappings");
    }
}
