//! Textured mesh construction: greedy quads grouped by [`BucketKey`]
//! and stamped with the right per-quad atlas UV rect.
//!
//! Greedy meshing has already run by the time this module is invoked.
//! All this code does is:
//!
//! 1. Look up each quad's [`FaceTextureInfo`] in the precomputed
//!    `face_buckets` map.
//! 2. Bucket the quads by [`BucketKey`].  Untextured quads go into the
//!    colour-only fallback mesh; static-textured quads go into one
//!    mesh per `(atlas_layer, render_layer)` bucket.
//! 3. Build one Bevy [`Mesh`] per bucket.  Per-quad atlas UV rect is
//!    baked into vertex UVs so the fragment shader can stay trivial.
//!
//! # Greedy-merge × atlas tiling limitation
//!
//! For textured quads spanning more than one block (`u_len > 1` or
//! `v_len > 1`), this builder **stretches** the texture across the
//! merged area instead of tiling it.  Tiling would require either
//! one-tile-per-layer atlas layout with REPEAT sampling, or a per-tile
//! UV-wrap shader — both are out of scope for the first textured
//! commit.  Visible effect on long unbroken textured surfaces: the
//! texture appears at lower spatial frequency.  Acceptable trade for
//! shipping textures end-to-end; revisit in a follow-up.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use dd40_core::block::{BlockId, BlockRegistry};
use dd40_texture_core::AtlasUv;

use crate::face_culling::FaceDir;
use crate::greedy_mesh::MergedQuad;
use crate::mesh_builder::MeshBuilder;
use crate::textures::bucket::{BucketKey, FaceBuckets, FaceTextureInfo, face_dir_to_face};

/// Indexes into [`FaceBuckets`] for a given [`FaceDir`].
fn face_dir_index(dir: FaceDir) -> usize {
    use dd40_texture_core::Face;
    let face = face_dir_to_face(dir);
    Face::ALL
        .iter()
        .position(|f| *f == face)
        .expect("Face::ALL contains every variant")
}

/// Hash/Eq-safe key for one UV sub-rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AtlasUvKey {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    base_layer: u32,
}

impl From<AtlasUv> for AtlasUvKey {
    fn from(uv: AtlasUv) -> Self {
        Self {
            min_x: uv.min.x.to_bits(),
            min_y: uv.min.y.to_bits(),
            max_x: uv.max.x.to_bits(),
            max_y: uv.max.y.to_bits(),
            base_layer: uv.base_layer,
        }
    }
}

/// Textured mesh split key: material bucket + base/overlay UV sub-rects.
///
/// Two faces must not share a mesh if either base or overlay UV rect differs,
/// even when they sample the same atlas layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TexturedBucketKey {
    bucket: BucketKey,
    uv: AtlasUvKey,
    overlay_uv: Option<AtlasUvKey>,
}

/// One Bevy mesh + the bucket it belongs to, ready to be paired with
/// the appropriate [`BlockAtlasMaterial`](super::material::BlockAtlasMaterial)
/// or [`StandardMaterial`] by the apply pass.
#[derive(Debug)]
pub struct BucketMesh {
    /// Which bucket produced this mesh.
    pub bucket: BucketKey,
    /// Atlas sub-rect for the base texture of this bucket.  Passed to
    /// the material so the fragment shader can wrap tile-space UVs
    /// into the rect.  Always [`AtlasUv::default`]-like (zeros) for
    /// untextured buckets, where it is ignored.
    pub uv_rect: AtlasUv,
    /// Atlas sub-rect for the overlay texture, when this bucket has
    /// one.  `None` mirrors `BucketKey::Static::overlay_layer == None`.
    pub overlay_uv_rect: Option<AtlasUv>,
    /// The actual mesh asset.  Always non-empty (empty buckets are
    /// filtered out before this struct is constructed).
    pub mesh: Mesh,
}

/// Splits a chunk's greedy-merged quads into one mesh per bucket.
///
/// `face_buckets` is the per-block face → bucket / atlas-UV table
/// produced by
/// [`compute_face_buckets`](super::bucket::compute_face_buckets).
/// `color_map` is the per-block linear-RGBA tint colour, used both for
/// the untextured fallback bucket (as the only colour) and for textured
/// buckets (multiplied into the sampled texel — matches Minecraft's
/// grass-tint convention).
///
/// Returns one [`BucketMesh`] per non-empty bucket.  Order is
/// unspecified.
pub fn build_chunk_bucket_meshes(
    chunk_origin_x: f32,
    chunk_origin_z: f32,
    quads: &[MergedQuad],
    face_buckets: &HashMap<BlockId, FaceBuckets>,
    color_map: &HashMap<BlockId, [f32; 4]>,
) -> Vec<BucketMesh> {
    let mut by_bucket: HashMap<TexturedBucketKey, Vec<(MergedQuad, AtlasUv, Option<AtlasUv>)>> =
        HashMap::new();
    let mut untextured: Vec<MergedQuad> = Vec::new();

    for quad in quads {
        let info = face_buckets
            .get(&quad.block_id)
            .map(|fb| fb[face_dir_index(quad.dir)])
            .unwrap_or(FaceTextureInfo::UNTEXTURED);
        match info {
            FaceTextureInfo {
                bucket: BucketKey::Untextured,
                ..
            } => {
                untextured.push(quad.clone());
            }
            FaceTextureInfo {
                bucket,
                uv: Some(uv),
                overlay_uv,
            } => {
                let key = TexturedBucketKey {
                    bucket,
                    uv: uv.into(),
                    overlay_uv: overlay_uv.map(Into::into),
                };
                by_bucket
                    .entry(key)
                    .or_default()
                    .push((quad.clone(), uv, overlay_uv));
            }
            FaceTextureInfo {
                bucket: _,
                uv: None,
                ..
            } => {
                // Should never happen: any non-Untextured bucket has a UV
                // by construction in `info_from_resolved`.  Fail safe to
                // the untextured bucket so we still render something.
                untextured.push(quad.clone());
            }
        }
    }

    let mut out = Vec::with_capacity(by_bucket.len() + 1);

    if !untextured.is_empty() {
        let mut builder = MeshBuilder::new(chunk_origin_x, chunk_origin_z);
        for quad in &untextured {
            let color = color_map
                .get(&quad.block_id)
                .copied()
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            builder.add_quad_with_color(quad, color);
        }
        if let Some(mesh) = builder.build() {
            out.push(BucketMesh {
                bucket: BucketKey::Untextured,
                uv_rect: AtlasUv::default(),
                overlay_uv_rect: None,
                mesh,
            });
        }
    }

    for (bucket_key, items) in by_bucket {
        // All items in this split bucket share both the material bucket and
        // UV sub-rect(s) by construction of `TexturedBucketKey`.
        let uv_rect = items[0].1;
        let overlay_uv_rect = items[0].2;
        let mesh = build_textured_mesh(chunk_origin_x, chunk_origin_z, &items, color_map);
        if let Some(mesh) = mesh {
            out.push(BucketMesh {
                bucket: bucket_key.bucket,
                uv_rect,
                overlay_uv_rect,
                mesh,
            });
        }
    }

    out
}

/// Builds a single textured mesh from a slice of `(quad, atlas_uv,
/// overlay_uv)` tuples that all belong to the same bucket.
///
/// Vertex UVs are written in **tile space** — one unit per block —
/// rather than baked into the atlas sub-rect.  The fragment shader
/// wraps with `fract()` and maps back into the sub-rect supplied via
/// [`BlockAtlasParams`](super::material::BlockAtlasParams), which is
/// what lets greedy-merged quads *tile* the texture per block instead
/// of stretching it across the merged extent.
fn build_textured_mesh(
    chunk_origin_x: f32,
    chunk_origin_z: f32,
    items: &[(MergedQuad, AtlasUv, Option<AtlasUv>)],
    color_map: &HashMap<BlockId, [f32; 4]>,
) -> Option<Mesh> {
    if items.is_empty() {
        return None;
    }

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(items.len() * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(items.len() * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(items.len() * 4);
    let mut uvs1: Vec<[f32; 2]> = Vec::with_capacity(items.len() * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(items.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(items.len() * 6);

    for (quad, _uv_rect, _overlay_rect) in items {
        let normal = quad.dir.normal();
        let color = color_map
            .get(&quad.block_id)
            .copied()
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let base = positions.len() as u32;

        let corners = crate::mesh_builder::quad_corners(quad, chunk_origin_x, chunk_origin_z);
        for c in &corners {
            positions.push(*c);
            normals.push(normal);
            colors.push(color);
        }

        // Tile-space UV: the canonical per-face unit-square pattern
        // scaled by the merged extent.  A 1×1 quad emits [0..1] (same
        // as before merging); a 3×2 quad emits [0..3]×[0..2], and the
        // shader's `fract` wraps each block to its own copy of the
        // texture.  UV1 (overlay) mirrors UV0 — they index the same
        // per-block tile and the shader applies independent sub-rect
        // remaps for base vs overlay.
        let pattern = uv_pattern_for(quad.dir);
        let u_len = quad.u_len as f32;
        let v_len = quad.v_len as f32;
        for unit in pattern {
            let tile = [unit[0] * u_len, unit[1] * v_len];
            uvs.push(tile);
            uvs1.push(tile);
        }

        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uvs1);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

/// Returns the four-corner UV pattern for a given face direction,
/// in the same vertex order as [`crate::mesh_builder::quad_corners`].
///
/// The pattern is chosen so that the source texture's `(0,0)`
/// (top-left in image space) appears at the **visual** top-left of
/// the rendered face when viewed from outside the block.  Without
/// this, side faces show the texture rotated 180° — most visibly
/// the grass-block sides render upside-down (green at the bottom).
fn uv_pattern_for(dir: FaceDir) -> [[f32; 2]; 4] {
    match dir {
        // All four side faces (X- and Z-aligned) share the same pattern.
        // `quad_corners` orders them bottom-then-top in world Y, so we
        // map the bottom two corners to the texture's bottom edge
        // (V = 1) and the top two corners to V = 0.
        FaceDir::PosX | FaceDir::NegX | FaceDir::PosZ | FaceDir::NegZ => {
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        }
        // Top face: viewed from +Y looking down, `quad_corners` puts
        // the high-Z corners first (FL/FR) and the low-Z corners last
        // (BR/BL).  Map high-Z → texture bottom edge (V = 1).
        FaceDir::PosY => [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        // Bottom face: rarely seen, no asymmetric bottom textures in
        // the vanilla palette.  Keep the historical pattern so existing
        // colour-only tests stay reproducible.
        FaceDir::NegY => [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    }
}

/// Re-used color extraction for callers that need to pre-collect the
/// per-block tint map before spawning an async mesh task (where
/// [`BlockRegistry`] is not `Send`-friendly to capture).
pub fn collect_color_map(registry: &BlockRegistry) -> HashMap<BlockId, [f32; 4]> {
    registry
        .iter()
        .map(|def| (def.id, crate::mesh_builder::linear_rgba(def.color)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use dd40_texture_core::AtlasId;

    fn quad(block_id: BlockId, dir: FaceDir) -> MergedQuad {
        MergedQuad {
            block_id,
            dir,
            layer: 0,
            u_start: 0,
            v_start: 0,
            u_len: 1,
            v_len: 1,
        }
    }

    fn buckets_for(block_id: BlockId, info: FaceTextureInfo) -> HashMap<BlockId, FaceBuckets> {
        let mut m = HashMap::new();
        m.insert(block_id, [info; 6]);
        m
    }

    #[test]
    fn empty_quads_produce_no_meshes() {
        let face_buckets = HashMap::new();
        let color_map = HashMap::new();
        let out = build_chunk_bucket_meshes(0.0, 0.0, &[], &face_buckets, &color_map);
        assert!(out.is_empty());
    }

    #[test]
    fn untextured_block_produces_single_untextured_bucket() {
        let id = BlockId(42);
        let mut color_map = HashMap::new();
        color_map.insert(id, [0.5, 0.5, 0.5, 1.0]);
        let face_buckets = HashMap::new();
        let q = [quad(id, FaceDir::PosY)];
        let out = build_chunk_bucket_meshes(0.0, 0.0, &q, &face_buckets, &color_map);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bucket, BucketKey::Untextured);
    }

    #[test]
    fn static_textured_block_produces_single_static_bucket() {
        let id = BlockId(7);
        let uv = AtlasUv {
            min: Vec2::new(0.0, 0.0),
            max: Vec2::new(0.25, 0.25),
            base_layer: 3,
        };
        let info = FaceTextureInfo {
            bucket: BucketKey::Static {
                atlas_id: AtlasId(0),
                atlas_layer: 3,
                render_layer: dd40_texture_core::RenderLayer::Opaque,
                tinted: false,
                overlay_layer: None,
            },
            uv: Some(uv),
            overlay_uv: None,
        };
        let face_buckets = buckets_for(id, info);
        let mut color_map = HashMap::new();
        color_map.insert(id, [1.0, 1.0, 1.0, 1.0]);
        let q = [quad(id, FaceDir::PosY)];
        let out = build_chunk_bucket_meshes(0.0, 0.0, &q, &face_buckets, &color_map);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bucket, info.bucket);
    }

    #[test]
    fn mixed_blocks_produce_one_mesh_per_bucket() {
        let untex = BlockId(1);
        let tex = BlockId(2);
        let uv = AtlasUv {
            min: Vec2::ZERO,
            max: Vec2::ONE,
            base_layer: 0,
        };
        let info = FaceTextureInfo {
            bucket: BucketKey::Static {
                atlas_id: AtlasId(0),
                atlas_layer: 0,
                render_layer: dd40_texture_core::RenderLayer::Opaque,
                tinted: false,
                overlay_layer: None,
            },
            uv: Some(uv),
            overlay_uv: None,
        };
        let face_buckets = buckets_for(tex, info);
        let mut color_map = HashMap::new();
        color_map.insert(untex, [0.5, 0.5, 0.5, 1.0]);
        color_map.insert(tex, [1.0, 1.0, 1.0, 1.0]);
        let q = [quad(untex, FaceDir::PosY), quad(tex, FaceDir::PosY)];
        let out = build_chunk_bucket_meshes(0.0, 0.0, &q, &face_buckets, &color_map);
        assert_eq!(out.len(), 2);
        let kinds: std::collections::HashSet<_> = out.iter().map(|b| b.bucket).collect();
        assert!(kinds.contains(&BucketKey::Untextured));
        assert!(kinds.contains(&info.bucket));
    }

    #[test]
    fn same_bucket_different_uv_rects_split_into_separate_meshes() {
        // Regression guard: multiple textures can share the same atlas layer
        // and render layer but occupy different UV sub-rects in that layer.
        // They must not be forced into one mesh/material pair, or one rect
        // will overwrite the other and some faces show the wrong texture.
        let a = BlockId(10);
        let b = BlockId(11);
        let shared_bucket = BucketKey::Static {
            atlas_id: AtlasId(0),
            atlas_layer: 0,
            render_layer: dd40_texture_core::RenderLayer::Opaque,
            tinted: false,
            overlay_layer: None,
        };
        let info_a = FaceTextureInfo {
            bucket: shared_bucket,
            uv: Some(AtlasUv {
                min: Vec2::new(0.0, 0.0),
                max: Vec2::new(0.5, 0.5),
                base_layer: 0,
            }),
            overlay_uv: None,
        };
        let info_b = FaceTextureInfo {
            bucket: shared_bucket,
            uv: Some(AtlasUv {
                min: Vec2::new(0.5, 0.0),
                max: Vec2::new(1.0, 0.5),
                base_layer: 0,
            }),
            overlay_uv: None,
        };
        let mut face_buckets = HashMap::new();
        face_buckets.insert(a, [info_a; 6]);
        face_buckets.insert(b, [info_b; 6]);
        let mut color_map = HashMap::new();
        color_map.insert(a, [1.0, 1.0, 1.0, 1.0]);
        color_map.insert(b, [1.0, 1.0, 1.0, 1.0]);

        let q = [quad(a, FaceDir::NegY), quad(b, FaceDir::NegY)];
        let out = build_chunk_bucket_meshes(0.0, 0.0, &q, &face_buckets, &color_map);
        assert_eq!(
            out.len(),
            2,
            "different UV sub-rects in same atlas layer must not share one mesh"
        );
        let has_rect = |min: Vec2, max: Vec2| {
            out.iter().any(|m| {
                (m.uv_rect.min - min).length_squared() < 1e-12
                    && (m.uv_rect.max - max).length_squared() < 1e-12
            })
        };
        assert!(has_rect(Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.5)));
        assert!(has_rect(Vec2::new(0.5, 0.0), Vec2::new(1.0, 0.5)));
    }

    #[test]
    fn merged_quad_emits_tile_space_uvs() {
        // Build a 3x2 merged quad on the +Y face and verify the UVs
        // span [0..3]×[0..2] in tile space rather than being baked
        // into the atlas sub-rect.  The shader-side `fract` is what
        // turns this into per-block tiling.
        use crate::greedy_mesh::MergedQuad;
        let id = BlockId(99);
        let rect = AtlasUv {
            min: Vec2::new(0.25, 0.0),
            max: Vec2::new(0.5, 0.25),
            base_layer: 0,
        };
        let q = MergedQuad {
            block_id: id,
            dir: FaceDir::PosY,
            layer: 0,
            u_start: 0,
            v_start: 0,
            u_len: 3,
            v_len: 2,
        };
        let mut color_map = HashMap::new();
        color_map.insert(id, [1.0, 1.0, 1.0, 1.0]);
        let mesh =
            build_textured_mesh(0.0, 0.0, &[(q, rect, None)], &color_map).expect("mesh built");
        let uvs = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .and_then(|a| match a {
                bevy::mesh::VertexAttributeValues::Float32x2(v) => Some(v.clone()),
                _ => None,
            })
            .expect("UV0");
        // PosY pattern is [0,1],[1,1],[1,0],[0,0]; scaled by (3, 2):
        let expected = [[0.0, 2.0], [3.0, 2.0], [3.0, 0.0], [0.0, 0.0]];
        for (got, want) in uvs.iter().zip(expected.iter()) {
            assert!(
                (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
                "got {got:?}, want {want:?}"
            );
        }
    }

    #[test]
    fn side_face_uv_patterns_put_texture_top_at_quad_top() {
        // `quad_corners` orders side faces bottom-then-top in world Y
        // (indices 0,1 are at the bottom; 2,3 at the top).  V = 0 is
        // the top of the texture, so the top corners must get V = 0.
        for dir in [FaceDir::PosX, FaceDir::NegX, FaceDir::PosZ, FaceDir::NegZ] {
            let p = uv_pattern_for(dir);
            assert_eq!(p[0][1], 1.0, "{dir:?} corner 0 (bottom) V");
            assert_eq!(p[1][1], 1.0, "{dir:?} corner 1 (bottom) V");
            assert_eq!(p[2][1], 0.0, "{dir:?} corner 2 (top) V");
            assert_eq!(p[3][1], 0.0, "{dir:?} corner 3 (top) V");
        }
    }

    #[test]
    fn top_face_uv_pattern_aligns_high_z_with_texture_bottom() {
        // `quad_corners` for PosY puts high-Z (front) corners first
        // (indices 0,1) and low-Z (back) corners last (2,3).
        // Texture's V = 1 is its bottom edge.
        let p = uv_pattern_for(FaceDir::PosY);
        assert_eq!(p[0][1], 1.0);
        assert_eq!(p[1][1], 1.0);
        assert_eq!(p[2][1], 0.0);
        assert_eq!(p[3][1], 0.0);
    }
}
