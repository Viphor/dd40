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
//! The resulting `Vec<MergedQuad>` is passed to `MeshBuilder` by the caller
//! (`systems`) to produce the final Bevy `Mesh`.

use dd40_core::{
    block::{BlockId, BlockRegistry},
    chunk::cache::ChunkCache,
    chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk},
};

use crate::{
    face_culling::{FaceDir, is_face_visible_lod},
    greedy_mesh::{MergedQuad, empty_mask, greedy_mesh_slice},
    lod::LodLevel,
};

// ── Public entry-point ────────────────────────────────────────────────────────

/// Generates all [`MergedQuad`]s for `chunk` at the given `lod` level.
///
/// The returned quads are in chunk-local coordinates (not world coordinates).
/// The caller is responsible for translating them to world space using
/// `MeshBuilder`.
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

        // The face_layer is the block-coordinate position of the layer being
        // rendered.  Since Y is always at full resolution, layer_idx IS the
        // block coordinate for PosY/NegY — no multiplication needed.
        //
        // For horizontal faces the layer axis is X (PosX/NegX) or Z (PosZ/NegZ),
        // which are still LoD-cell-sampled.  Positive horizontal faces sit at the
        // far edge of the cell: `layer_idx * step + (step-1)` so that
        // `quad_corners` places the face at (layer_idx+1)*step — the cell boundary.
        // Negative horizontal faces sit at the near edge: `layer_idx * step`.
        let face_layer = match dir {
            FaceDir::PosX | FaceDir::PosZ => layer_idx * step + (step - 1),
            FaceDir::PosY | FaceDir::NegY => layer_idx, // Y is block-coordinate directly
            FaceDir::NegX | FaceDir::NegZ => layer_idx * step,
        };

        greedy_mesh_slice(&mut mask, u_cells, v_cells, dir, face_layer, out);
    }

    // Scale U and V from LoD-cell indices to block coordinates.
    // Y is already in block coordinates for all directions (full resolution).
    // X and Z are LoD-sampled and need to be multiplied by `step`.
    //
    // U is always an X or Z axis (never Y), so it always needs scaling.
    // V is Y for horizontal faces (no scaling) and Z for vertical faces (scale).
    if step > 1 {
        for q in out[start_idx..].iter_mut() {
            q.u_start *= step;
            q.u_len *= step;
            match dir {
                FaceDir::PosY | FaceDir::NegY => {
                    // V = Z (cell-sampled).
                    q.v_start *= step;
                    q.v_len *= step;
                }
                _ => {} // V = Y (full resolution, already in block coords)
            }
        }
    }
}

/// Fills the 2-D visibility mask for a given `(dir, layer_idx)` pair.
///
/// Each cell `mask[u][v]` is set to `Some(block_id)` when the corresponding
/// block face is visible and the block is non-air, or left as `None` when it
/// is occluded or the block is air / non-renderable.
///
/// # LoD-aware neighbour check
///
/// Face visibility is determined by checking the block `step` positions away
/// in direction `dir` — not just 1 position.  At LoD0 (step = 1) this is
/// identical to a standard adjacency check.  At higher LoD levels it ensures
/// that the checked neighbour is the representative of the *adjacent LoD
/// cell*, so chunk-boundary faces are not incorrectly culled when the
/// intervening blocks are solid but the LoD-cell boundary opens to air.
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

            // Block must be non-air and renderable.
            let Some(block) = chunk.get(lx, ly, lz) else { continue };
            if block.block_id == BlockId::AIR || !registry.is_renderable(&block) {
                continue;
            }

            // Face visibility: the neighbour is checked `face_step` blocks away.
            //
            // For horizontal faces (PosX/NegX/PosZ/NegZ) the face normal runs
            // along X or Z; we check `step` positions away so the neighbour is
            // the adjacent LoD-cell representative in the next chunk.
            //
            // For vertical faces (PosY/NegY) the face normal runs along Y which
            // is at full block resolution; the relevant neighbour is always the
            // immediately adjacent block 1 position away.
            let face_step = match dir {
                FaceDir::PosY | FaceDir::NegY => 1,
                _ => step,
            };
            if is_face_visible_lod(chunk, lx, ly, lz, dir, face_step, registry, cache) {
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
/// Y is always kept at **full block resolution** regardless of `step`:
///
/// - For horizontal faces (`PosX`/`NegX`/`PosZ`/`NegZ`), Y is the V axis
///   and is not divided by `step`.  This ensures side-face extents precisely
///   match the solid-block height, not the coarser LoD-cell height.
/// - For vertical faces (`PosY`/`NegY`), Y is the layer axis and is likewise
///   not divided by `step`.  This ensures the top/bottom face appears exactly
///   at the surface block's Y, matching the height seen at LoD0.
///
/// X and Z are divided by `step` in all cases.
pub(crate) fn dir_extents(dir: FaceDir, step: usize) -> (usize, usize, usize) {
    let nx = CHUNK_SIZE_X / step;
    let nz = CHUNK_SIZE_Z / step;
    match dir {
        FaceDir::PosX | FaceDir::NegX => (nx, nz, CHUNK_SIZE_Y),
        FaceDir::PosY | FaceDir::NegY => (CHUNK_SIZE_Y, nx, nz),
        FaceDir::PosZ | FaceDir::NegZ => (nz, nx, CHUNK_SIZE_Y),
    }
}

/// Converts a sampled `(layer_idx, u, v)` cell to chunk-local `(lx, ly, lz)`.
///
/// Y is always at full block resolution — `layer_idx` IS the block Y for
/// vertical faces, and `v` IS the block Y for horizontal faces.  X and Z are
/// in LoD-cell space and are multiplied by `step`.
///
/// Axis mappings:
///
/// | `dir`            | layer →        | u →     | v →        |
/// |------------------|----------------|---------|------------|
/// | `PosX` / `NegX`  | X = layer×step | Z = u×step | Y = v (direct) |
/// | `PosY` / `NegY`  | Y = layer (direct) | X = u×step | Z = v×step |
/// | `PosZ` / `NegZ`  | Z = layer×step | X = u×step | Y = v (direct) |
fn cell_to_local(
    dir: FaceDir,
    layer_idx: usize,
    u: usize,
    v: usize,
    step: usize,
) -> (usize, usize, usize) {
    let us = u * step;
    match dir {
        FaceDir::PosX | FaceDir::NegX => (layer_idx * step, v, us),  // layer=X, U=Z, V=Y direct
        FaceDir::PosY | FaceDir::NegY => (us, layer_idx, v * step),  // layer=Y direct, U=X, V=Z
        FaceDir::PosZ | FaceDir::NegZ => (us, v, layer_idx * step),  // layer=Z, U=X, V=Y direct
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
        // PosY, step=2: layer=Y direct (layer_idx=3 → ly=3), u→X*2=4, v→Z*2=8
        let (lx, ly, lz) = cell_to_local(FaceDir::PosY, 3, 2, 4, 2);
        assert_eq!((lx, ly, lz), (4, 3, 8));
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
        // PosY: layer (Y) is always full resolution; only U (X) and V (Z) are halved.
        let (l, u, v) = dir_extents(FaceDir::PosY, 2);
        assert_eq!(l, CHUNK_SIZE_Y, "PosY layer (Y) is full resolution");
        assert_eq!(u, CHUNK_SIZE_X / 2);
        assert_eq!(v, CHUNK_SIZE_Z / 2);
    }

    #[test]
    fn dir_extents_step4_quarters_counts() {
        // PosX: layer (X) and U (Z) are quartered; V (Y) is full resolution.
        let (l, u, v) = dir_extents(FaceDir::PosX, 4);
        assert_eq!(l, CHUNK_SIZE_X / 4);
        assert_eq!(u, CHUNK_SIZE_Z / 4);
        assert_eq!(v, CHUNK_SIZE_Y, "V (Y axis) must be full resolution for horizontal faces");
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

    // ── Cross-chunk boundary face culling ─────────────────────────────────────

    /// Regression: a block at the +X edge of chunk A should NOT emit a +X face
    /// when the block in chunk B at that boundary is solid.
    ///
    /// Without a populated neighbour cache the face was always treated as
    /// visible (air), causing chunk seams with extra faces showing.
    #[test]
    fn boundary_face_culled_when_neighbour_chunk_is_solid() {
        use dd40_core::block::{Block, BlockDefinition, BlockId};

        let stone = BlockId(1);
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(stone, "stone")
                .with_solid(true)
                .with_renderable(true)
                .with_color(bevy::color::Color::WHITE),
        );

        // Chunk A: stone block at its +X boundary (lx = CHUNK_SIZE_X − 1).
        let mut chunk_a = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk_a.set(CHUNK_SIZE_X - 1, 0, 0, Block::new(stone));

        // Chunk B (neighbour in +X direction): solid block at its -X boundary (lx = 0).
        let mut chunk_b = Chunk::new(ChunkPos::new(1, 0, 0));
        for ly in 0..CHUNK_SIZE_Y {
            for lz in 0..CHUNK_SIZE_Z {
                chunk_b.set(0, ly, lz, Block::new(stone));
            }
        }

        // With an empty neighbour cache the +X face MUST appear (conservative fallback).
        let empty_cache = ChunkCache::default();
        let quads_no_neighbour = build_chunk_quads(&chunk_a, LodLevel::Lod0, &registry, &empty_cache);
        assert!(
            quads_no_neighbour.iter().any(|q| q.dir == FaceDir::PosX),
            "with empty neighbour cache +X boundary face should be visible (conservative)"
        );

        // With chunk B in the neighbour cache the +X face MUST be culled.
        let mut neighbour_cache = ChunkCache::default();
        neighbour_cache.insert(chunk_b);
        let quads_with_neighbour =
            build_chunk_quads(&chunk_a, LodLevel::Lod0, &registry, &neighbour_cache);
        assert!(
            !quads_with_neighbour.iter().any(|q| q.dir == FaceDir::PosX),
            "+X boundary face must be culled when the neighbour chunk has a solid block there"
        );
    }

    // ── LOD chunk-boundary face culling ──────────────────────────────────────

    /// Regression: at LoD1, a block at the +X penultimate layer (lx = 14)
    /// must emit a +X face when the adjacent chunk is empty, even though the
    /// immediately adjacent block (lx = 15) is solid.
    ///
    /// The old code used a +1 neighbour check, so it saw the solid block at
    /// lx = 15 and wrongly culled the cliff face.  The fix uses +step so the
    /// neighbour is the adjacent LoD-cell representative in the next chunk.
    #[test]
    fn lod1_cliff_face_visible_at_chunk_boundary() {
        use dd40_core::block::{Block, BlockDefinition, BlockId};

        let stone = BlockId(1);
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(stone, "stone")
                .with_solid(true)
                .with_renderable(true)
                .with_color(bevy::color::Color::WHITE),
        );

        // lx=14 and lx=15 are both solid; the old +1 check (neighbour at 15)
        // would cull the face of the LoD1 representative at lx=14.
        let mut chunk_a = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk_a.set(14, 0, 0, Block::new(stone));
        chunk_a.set(15, 0, 0, Block::new(stone));

        // Chunk B (cx=1) is all-air. Inserting it in the cache confirms the
        // boundary is known — without this, a missing-chunk fallback (→ treat
        // as air) would also make the face visible, masking the real fix.
        let mut cache = ChunkCache::default();
        cache.insert(Chunk::new(ChunkPos::new(1, 0, 0)));

        let quads = build_chunk_quads(&chunk_a, LodLevel::Lod1, &registry, &cache);
        let pos_x_quad = quads.iter().find(|q| q.dir == FaceDir::PosX);
        assert!(
            pos_x_quad.is_some(),
            "LoD1 +X cliff face must be visible even when lx=15 is solid \
             (neighbour check must use +step, not +1)"
        );
        // At LoD1 (step=2), cell 7 covers lx=14,15.  face_layer = 14 + (2-1) = 15,
        // so quad_corners places the face at x = chunk_ox + 15 + 1 = 16 (chunk edge).
        assert_eq!(
            pos_x_quad.unwrap().layer,
            15,
            "LoD1 +X cliff quad layer must be 15 (= lx + step − 1) so the face \
             appears at the LoD-cell boundary, not 1 block inside the cliff"
        );

        // Inverse: when the LoD1 representative in chunk B (lx=0) is solid,
        // the face must be culled.
        let solid_b = {
            let mut b = Chunk::new(ChunkPos::new(1, 0, 0));
            b.set(0, 0, 0, Block::new(stone));
            b
        };
        let mut cache_solid = ChunkCache::default();
        cache_solid.insert(solid_b);
        let quads_solid =
            build_chunk_quads(&chunk_a, LodLevel::Lod1, &registry, &cache_solid);
        assert!(
            !quads_solid.iter().any(|q| q.dir == FaceDir::PosX),
            "LoD1 +X face must be culled when the adjacent LoD-cell representative is solid"
        );
    }

    // ── LOD quad position scaling ─────────────────────────────────────────────

    /// Regression: at LoD1/LoD2, greedy-meshed quads must have `u_start` and
    /// `v_start` in *block* coordinates, not *sampled-cell* coordinates.
    ///
    /// Before the fix, only `u_len`/`v_len` were multiplied by `step`.
    /// This caused side faces to render at the wrong (too low) Y position
    /// and top/bottom faces to render at the wrong X/Z position — all
    /// proportional to how deep into the chunk the block sat.
    ///
    /// Note: PosX/PosZ side faces use V at full Y resolution (v_step=1) so
    /// v_start is already in block coordinates without any scaling.
    #[test]
    fn lod_quads_have_block_space_start_positions() {
        use dd40_core::block::{Block, BlockDefinition, BlockId};

        let stone = BlockId(1);
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(stone, "stone")
                .with_solid(true)
                .with_renderable(true)
                .with_color(bevy::color::Color::WHITE),
        );

        // Place one isolated stone block at (lx=4, ly=8, lz=4).
        // At LoD1 (step=2) this is sampled at cell v=4 in the Y direction.
        // After scaling v_start must be 8 (block Y), not 4 (cell index).
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set(4, 8, 4, Block::new(stone));

        let cache = ChunkCache::default();

        let quads_lod0 = build_chunk_quads(&chunk, LodLevel::Lod0, &registry, &cache);
        let quads_lod1 = build_chunk_quads(&chunk, LodLevel::Lod1, &registry, &cache);

        // PosY: layer must be the block's own Y, not the top of the LoD cell.
        // Both LoD0 and LoD1 should produce layer=8 (face at Y=9).
        let top_lod0 = quads_lod0.iter().find(|q| q.dir == FaceDir::PosY)
            .expect("PosY face must exist at LoD0");
        assert_eq!(top_lod0.layer, 8, "LoD0 PosY layer must be the block Y (8)");

        let top_lod1 = quads_lod1.iter().find(|q| q.dir == FaceDir::PosY)
            .expect("PosY face must exist at LoD1");
        assert_eq!(
            top_lod1.layer, 8,
            "LoD1 PosY layer must be 8 (face at Y=9, top of the actual block)"
        );

        // PosX side face: v_start (block Y coordinate of the bottom edge) must be
        // in block space, not cell space.
        let side_lod0 = quads_lod0.iter().find(|q| q.dir == FaceDir::PosX);
        let side_lod1 = quads_lod1.iter().find(|q| q.dir == FaceDir::PosX);
        if let (Some(s0), Some(s1)) = (side_lod0, side_lod1) {
            assert_eq!(
                s0.v_start, 8,
                "LoD0 PosX v_start must be block Y (8)"
            );
            assert_eq!(
                s1.v_start, 8,
                "LoD1 PosX v_start must be block Y (8), not cell index (4)"
            );
        }
    }

    /// Regression: at LoD1, a single surface block must produce a side face that
    /// ends precisely at the terrain top — not 1 block above it.
    ///
    /// With the old step-based V downsampling, the PosX mask cell for one solid
    /// block at `ly=8` (cell v=4) merged into v_len=1 cell, which scaled to
    /// v_len=2 blocks, making the cliff face span Y=[8,10] when it should span
    /// Y=[8,9].  The fix keeps V at full block resolution for horizontal faces.
    #[test]
    fn lod1_single_surface_block_side_face_exact_height() {
        use dd40_core::block::{Block, BlockDefinition, BlockId};

        let stone = BlockId(1);
        let mut registry = BlockRegistry::new();
        registry.register_without_event(
            BlockDefinition::new(stone, "stone")
                .with_solid(true)
                .with_renderable(true)
                .with_color(bevy::color::Color::WHITE),
        );

        // Single surface block at ly=8 (ly=9 is air). Placed at the +X edge
        // so lx=14 and lx=15 are both solid — the LoD1 culling test requires the
        // +step neighbour, not the immediate neighbour.
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set(14, 8, 0, Block::new(stone));
        chunk.set(15, 8, 0, Block::new(stone)); // fills the other half of the LoD1 X-cell

        // All-air neighbour chunk so the +X boundary face is visible.
        let mut cache = ChunkCache::default();
        cache.insert(Chunk::new(ChunkPos::new(1, 0, 0)));

        let quads = build_chunk_quads(&chunk, LodLevel::Lod1, &registry, &cache);
        let pos_x = quads.iter().find(|q| q.dir == FaceDir::PosX)
            .expect("PosX face must be visible at LoD1");

        assert_eq!(
            pos_x.v_start, 8,
            "PosX v_start must be block Y (8)"
        );
        assert_eq!(
            pos_x.v_len, 1,
            "PosX v_len must be 1 for a single surface block — cliff face must not overshoot terrain"
        );
    }
}
