//! Textured-block rendering pipeline (feature `textures`).
//!
//! Holds the custom [`Material`](bevy::pbr::Material) and (in
//! follow-up commits) the bucket-split greedy meshing that drives it.
//!
//! See [`crate::RendererPlugin`] for how this module is wired in.

pub mod bucket;
pub mod material;

pub use bucket::{BucketKey, FaceBuckets, compute_face_buckets};
pub use material::{
    BLOCK_ATLAS_SHADER_HANDLE, BlockAtlasMaterial, BlockAtlasMaterialPlugin, BlockAtlasParams,
};

/// Returns `true` once the bucket-split mesh code is wired into the
/// renderer's task pipeline.  Bucket precomputation alone (`bucket.rs`)
/// does not flip this — the mesh-split commit will.
pub const fn pipeline_ready() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_is_not_ready_yet() {
        assert!(!pipeline_ready());
    }
}
