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

/// One Bevy mesh + the bucket it belongs to, ready to be paired with
/// the appropriate [`BlockAtlasMaterial`](super::material::BlockAtlasMaterial)
/// or [`StandardMaterial`] by the apply pass.
#[derive(Debug)]
pub struct BucketMesh {
    /// Which bucket produced this mesh.
    pub bucket: BucketKey,
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
    let mut by_bucket: HashMap<BucketKey, Vec<(MergedQuad, AtlasUv)>> = HashMap::new();
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
            } => {
                by_bucket
                    .entry(bucket)
                    .or_default()
                    .push((quad.clone(), uv));
            }
            FaceTextureInfo {
                bucket: _,
                uv: None,
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
                mesh,
            });
        }
    }

    for (bucket, items) in by_bucket {
        let mesh = build_textured_mesh(chunk_origin_x, chunk_origin_z, &items, color_map);
        if let Some(mesh) = mesh {
            out.push(BucketMesh { bucket, mesh });
        }
    }

    out
}

/// Builds a single textured mesh from a slice of `(quad, atlas_uv)`
/// pairs that all belong to the same bucket.
///
/// The atlas UV rect is baked into vertex UVs by remapping the
/// per-quad unit-square corners `(0,0)..(1,1)` into `uv.min..uv.max`.
/// This means the fragment shader only ever samples within the tile —
/// no per-fragment rect math required.
fn build_textured_mesh(
    chunk_origin_x: f32,
    chunk_origin_z: f32,
    items: &[(MergedQuad, AtlasUv)],
    color_map: &HashMap<BlockId, [f32; 4]>,
) -> Option<Mesh> {
    if items.is_empty() {
        return None;
    }

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(items.len() * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(items.len() * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(items.len() * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(items.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(items.len() * 6);

    for (quad, uv_rect) in items {
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

        // Same per-quad UV pattern as `MeshBuilder` — (0,0) (1,0) (1,1)
        // (0,1) — remapped into the atlas tile rect.
        let pattern = [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for uv in pattern {
            uvs.push(remap_uv(uv, uv_rect));
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
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

/// Lerps a unit-square corner into the atlas tile rect.
fn remap_uv(unit: [f32; 2], rect: &AtlasUv) -> [f32; 2] {
    [
        rect.min.x + unit[0] * (rect.max.x - rect.min.x),
        rect.min.y + unit[1] * (rect.max.y - rect.min.y),
    ]
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
            },
            uv: Some(uv),
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
            },
            uv: Some(uv),
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
    fn remap_uv_endpoints_match_rect_corners() {
        let rect = AtlasUv {
            min: Vec2::new(0.25, 0.5),
            max: Vec2::new(0.5, 0.75),
            base_layer: 0,
        };
        assert_eq!(remap_uv([0.0, 0.0], &rect), [0.25, 0.5]);
        assert_eq!(remap_uv([1.0, 0.0], &rect), [0.5, 0.5]);
        assert_eq!(remap_uv([1.0, 1.0], &rect), [0.5, 0.75]);
        assert_eq!(remap_uv([0.0, 1.0], &rect), [0.25, 0.75]);
    }
}
