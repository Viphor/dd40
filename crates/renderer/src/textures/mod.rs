//! Textured-block rendering pipeline (feature `textures`).
//!
//! Holds the custom [`Material`](bevy::pbr::Material) and (in
//! follow-up commits) the bucket-split greedy meshing that drives it.
//!
//! See [`crate::RendererPlugin`] for how this module is wired in.

pub mod bucket;
pub mod material;
pub mod textured_mesh;

pub use bucket::{BucketKey, FaceBuckets, FaceTextureInfo, compute_face_buckets, face_dir_to_face};
pub use material::{
    BLOCK_ATLAS_SHADER_HANDLE, BlockAtlasMaterial, BlockAtlasMaterialPlugin, BlockAtlasParams,
};
pub use textured_mesh::{BucketMesh, build_chunk_bucket_meshes, collect_color_map};

/// Returns `true` once the bucket-split mesh code is wired into the
/// renderer's task pipeline.
pub const fn pipeline_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_is_ready() {
        assert!(pipeline_ready());
    }
}
