//! Full-chunk mesh generation combining face culling and greedy meshing.
//!
//! The main entry-point is [`build_chunk_quads`], which drives the entire
//! meshing pipeline for a single chunk at a chosen [`LodLevel`]:
//!
//! 1. **Downsampling** — at LOD1 and LOD2 the block data is sampled at a
//!    coarser step (every 2nd or 4th block), reducing the effective resolution.
//! 2. **Face culling** — for each sampled block position the six neighbouring
//!    blocks are checked; only visible faces are retained.
//! 3. **Greedy meshing** — per face direction, per layer, adjacent visible
//!    faces of the same [`BlockId`] are merged into maximal rectangles
//!    ([`MergedQuad`]s).
//!
//! The resulting `Vec<MergedQuad>` is passed to [`MeshBuilder`] by the caller
//! ([`systems`]) to produce the final Bevy [`Mesh`].

use dd40_core::{
    block::{BlockId, BlockRegistry},
    chunk::cache::ChunkCache,
    chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk},
};

use crate::{
    face_culling::{FaceDir, visible_faces},
    greedy_mesh::{MergedQuad, empty_mask, greedy_mesh_slice},
    lod::LodLevel,
};

// ── Public entry-point ────────────────────────────────────────────────────────

/// Generates all [`MergedQuad`]s for `chunk` at the given `lod` level.
///
/// The returned quads are in chunk-local coordinates (not world coordinates).
/// The caller is responsible for translating them to world space using
/// [`MeshBuilder`].
///
/// # Arguments
///
/// * `chunk`    — the chunk to mesh
/// * `lod`      — level of detail controlling the block-sampling step size
/// * `registry` — block registry used for solidity / renderability checks
/// * `cache`    — chunk cache used for cross-boundary face-culling lookups
///
/// # LOD downsampling
///
/// At [`LodLevel::Lod1`] blocks are sampled every 2 positions; at
/// [`LodLevel::Lod2`] every 4.  Each sampled block conceptually represents a
/// `step × step × step` voxel, so the emitted quads have `u_len` and `v_len`
/// scaled up by `step`.  This keeps the rendered surface area correct while
/// reducing triangle count.
pub fn build_chunk_quads(
    chunk: &Chunk,
    lod: LodLevel,
    registry: &BlockRegistry,
    cache: &ChunkCache,
) -> Vec<MergedQuad> {
    let step = lod.step();
    let mut quads = Vec::new();

    for dir in FaceDir::ALL {
        build_direction(chunk, step, dir, registry, cache, &mut quads);
    }

    quads
}

// ── Per-direction meshing ─────────────────────────────────────────────────────

/// Runs the face-cull → greedy-mesh pipeline for a single face direction.
///
/// For each layer along the face-normal axis a 2-D visibility mask is built,
/// then greedy meshing merges adjacent same-type cells into [`MergedQuad`]s.
/// When `step > 1` the quads' `u_len` and `v_len` are scaled up so the
/// rendered surface covers the correct world area.
fn build_direction(
    chunk: &Chunk,
    step: usize,
    dir: FaceDir,
    registry: &BlockRegistry,
    cache: &ChunkCache,
    out: &mut Vec<MergedQuad>,
) {
    let (layer_count, u_cells, v_cells) = dir_extents(dir, step);

    // Record the output length before we append quads for this direction so
    // we can scale only the quads we add here (not earlier directions).
    let start_idx = out.len();

    for layer_idx in 0..layer_count {
        let mut mask = empty_mask(u_cells, v_cells);
        fill_mask(chunk, step, dir, layer_idx, registry, cache, &mut mask);
        // Pass the un-scaled layer coordinate; scaling is applied below.
        greedy_mesh_slice(&mut mask, u_cells, v_cells, dir, layer_idx * step, out);
    }

    // Scale tangent extents by `step` so each quad covers the right surface
    // area at reduced LOD resolution.
    if step > 1 {
        for q in out[start_idx..].iter_mut() {
            q.u_len *= step;
            q.v_len *= step;
        }
    }
}

/// Fills the 2-D visibility mask for a given `(dir, layer_idx)` pair.
///
/// Each cell `mask[u][v]` is set to `Some(block_id)` when the corresponding
/// block face is visible and the block is non-air, or left as `None` when it
/// is occluded or the block is air / non-renderable.
///
/// # Arguments
///
/// * `chunk`      — source chunk
/// * `step`       — LOD sampling step
/// * `dir`        — face direction being processed
/// * `layer_idx`  — index of the current layer in *sampled* coordinates
///   (multiply by `step` to get chunk-local layer coordinate)
/// * `registry`   — block registry
/// * `cache`      — chunk cache for cross-boundary lookups
/// * `mask`       — output mask to fill; must already be sized `u_cells × v_cells`
fn fill_mask(
    chunk: &Chunk,
    step: usize,
    dir: FaceDir,
    layer_idx: usize,
    registry: &BlockRegistry,
    cache: &ChunkCache,
    mask: &mut [Vec<Option<BlockId>>],
) {
    let (_, u_cells, v_cells) = dir_extents(dir, step);

    #[allow(clippy::needless_range_loop)]
    // u and v are used for both cell_to_local and mask indexing
    for u in 0..u_cells {
        for v in 0..v_cells {
            let (lx, ly, lz) = cell_to_local(dir, layer_idx, u, v, step);

            // Skip if out of chunk bounds (can happen at step > 1 near edges).
            if lx >= CHUNK_SIZE_X || ly >= CHUNK_SIZE_Y || lz >= CHUNK_SIZE_Z {
                continue;
            }

            let faces = visible_faces(chunk, lx, ly, lz, registry, cache);
            if faces.is_visible(dir)
                && let Some(block) = chunk.get(lx, ly, lz)
                && block.block_id != BlockId::AIR
            {
                mask[u][v] = Some(block.block_id);
            }
        }
    }
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

/// Returns `(layer_count, u_cells, v_cells)` for `dir` at the given `step`.
///
/// | `dir`            | layer axis | U axis | V axis |
/// |------------------|------------|--------|--------|
/// | `PosX` / `NegX`  | X          | Z      | Y      |
/// | `PosY` / `NegY`  | Y          | X      | Z      |
/// | `PosZ` / `NegZ`  | Z          | X      | Y      |
///
/// All counts are divided by `step` (integer division), so they represent the
/// number of *samples* along each axis.
pub(crate) fn dir_extents(dir: FaceDir, step: usize) -> (usize, usize, usize) {
    let nx = CHUNK_SIZE_X / step;
    let ny = CHUNK_SIZE_Y / step;
    let nz = CHUNK_SIZE_Z / step;
    match dir {
        FaceDir::PosX | FaceDir::NegX => (nx, nz, ny),
        FaceDir::PosY | FaceDir::NegY => (ny, nx, nz),
        FaceDir::PosZ | FaceDir::NegZ => (nz, nx, ny),
    }
}

/// Converts a sampled `(layer_idx, u, v)` cell to chunk-local `(lx, ly, lz)`.
///
/// Each index is multiplied by `step` to obtain the actual chunk-local
/// coordinate.
///
/// Axis mappings:
///
/// | `dir`            | layer → | u → | v → |
/// |------------------|---------|-----|-----|
/// | `PosX` / `NegX`  | X       | Z   | Y   |
/// | `PosY` / `NegY`  | Y       | X   | Z   |
/// | `PosZ` / `NegZ`  | Z       | X   | Y   |
fn cell_to_local(
    dir: FaceDir,
    layer_idx: usize,
    u: usize,
    v: usize,
    step: usize,
) -> (usize, usize, usize) {
    let l = layer_idx * step;
    let us = u * step;
    let vs = v * step;
    match dir {
        FaceDir::PosX | FaceDir::NegX => (l, vs, us), // layer=X, U=Z, V=Y
        FaceDir::PosY | FaceDir::NegY => (us, l, vs), // layer=Y, U=X, V=Z
        FaceDir::PosZ | FaceDir::NegZ => (us, vs, l), // layer=Z, U=X, V=Y
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::{
        block::BlockRegistry,
        chunk::cache::ChunkCache,
        chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk, ChunkPos},
    };

    fn air_registry() -> BlockRegistry {
        BlockRegistry::new()
    }

    // ── cell_to_local ─────────────────────────────────────────────────────────

    #[test]
    fn cell_to_local_pos_y_step1() {
        // PosY: layer→Y, u→X, v→Z
        let (lx, ly, lz) = cell_to_local(FaceDir::PosY, 5, 3, 7, 1);
        assert_eq!((lx, ly, lz), (3, 5, 7));
    }

    #[test]
    fn cell_to_local_pos_x_step1() {
        // PosX: layer→X, u→Z, v→Y
        let (lx, ly, lz) = cell_to_local(FaceDir::PosX, 2, 4, 8, 1);
        assert_eq!((lx, ly, lz), (2, 8, 4));
    }

    #[test]
    fn cell_to_local_pos_z_step1() {
        // PosZ: layer→Z, u→X, v→Y
        let (lx, ly, lz) = cell_to_local(FaceDir::PosZ, 6, 1, 9, 1);
        assert_eq!((lx, ly, lz), (1, 9, 6));
    }

    #[test]
    fn cell_to_local_step2_scales_correctly() {
        // PosY, step=2: layer→Y*2=6, u→X*2=4, v→Z*2=8
        let (lx, ly, lz) = cell_to_local(FaceDir::PosY, 3, 2, 4, 2);
        assert_eq!((lx, ly, lz), (4, 6, 8));
    }

    #[test]
    fn cell_to_local_neg_x_same_as_pos_x() {
        // NegX uses the same axis mapping as PosX
        let pos = cell_to_local(FaceDir::PosX, 1, 2, 3, 1);
        let neg = cell_to_local(FaceDir::NegX, 1, 2, 3, 1);
        assert_eq!(pos, neg);
    }

    // ── dir_extents ───────────────────────────────────────────────────────────

    #[test]
    fn dir_extents_pos_y_step1() {
        // PosY: layer=CHUNK_SIZE_Y, u=CHUNK_SIZE_X, v=CHUNK_SIZE_Z
        let (l, u, v) = dir_extents(FaceDir::PosY, 1);
        assert_eq!(l, CHUNK_SIZE_Y);
        assert_eq!(u, CHUNK_SIZE_X);
        assert_eq!(v, CHUNK_SIZE_Z);
    }

    #[test]
    fn dir_extents_pos_x_step1() {
        // PosX: layer=CHUNK_SIZE_X, u=CHUNK_SIZE_Z, v=CHUNK_SIZE_Y
        let (l, u, v) = dir_extents(FaceDir::PosX, 1);
        assert_eq!(l, CHUNK_SIZE_X);
        assert_eq!(u, CHUNK_SIZE_Z);
        assert_eq!(v, CHUNK_SIZE_Y);
    }

    #[test]
    fn dir_extents_pos_z_step1() {
        // PosZ: layer=CHUNK_SIZE_Z, u=CHUNK_SIZE_X, v=CHUNK_SIZE_Y
        let (l, u, v) = dir_extents(FaceDir::PosZ, 1);
        assert_eq!(l, CHUNK_SIZE_Z);
        assert_eq!(u, CHUNK_SIZE_X);
        assert_eq!(v, CHUNK_SIZE_Y);
    }

    #[test]
    fn dir_extents_step2_halves_counts() {
        let (l, u, v) = dir_extents(FaceDir::PosY, 2);
        assert_eq!(l, CHUNK_SIZE_Y / 2);
        assert_eq!(u, CHUNK_SIZE_X / 2);
        assert_eq!(v, CHUNK_SIZE_Z / 2);
    }

    #[test]
    fn dir_extents_step4_quarters_counts() {
        let (l, u, v) = dir_extents(FaceDir::PosX, 4);
        assert_eq!(l, CHUNK_SIZE_X / 4);
        assert_eq!(u, CHUNK_SIZE_Z / 4);
        assert_eq!(v, CHUNK_SIZE_Y / 4);
    }

    // ── Empty chunk produces no quads ─────────────────────────────────────────

    #[test]
    fn empty_chunk_no_quads_lod0() {
        let registry = air_registry();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        let quads = build_chunk_quads(&chunk, LodLevel::Lod0, &registry, &cache);
        assert!(quads.is_empty(), "all-air chunk should produce no quads");
    }

    #[test]
    fn empty_chunk_no_quads_lod1() {
        let registry = air_registry();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        let quads = build_chunk_quads(&chunk, LodLevel::Lod1, &registry, &cache);
        assert!(quads.is_empty());
    }

    #[test]
    fn empty_chunk_no_quads_lod2() {
        let registry = air_registry();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        let quads = build_chunk_quads(&chunk, LodLevel::Lod2, &registry, &cache);
        assert!(quads.is_empty());
    }

    // ── LOD step sizes ────────────────────────────────────────────────────────

    #[test]
    fn lod_step_values() {
        assert_eq!(LodLevel::Lod0.step(), 1);
        assert_eq!(LodLevel::Lod1.step(), 2);
        assert_eq!(LodLevel::Lod2.step(), 4);
    }
}
