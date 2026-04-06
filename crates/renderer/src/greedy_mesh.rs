//! Greedy meshing for axis-aligned chunk slices.
//!
//! For each of the six face directions the algorithm iterates over every
//! axis-aligned layer (slice) perpendicular to that direction and merges
//! adjacent quads that share the same [`BlockId`] into maximal rectangles.
//! This dramatically reduces the triangle count compared to emitting one quad
//! per visible block face.
//!
//! # Algorithm overview
//!
//! For a given face direction and layer index:
//!
//! 1. Build a 2-D mask of `Option<BlockId>` — `Some(id)` where the face is
//!    visible, `None` where it is occluded or air.
//! 2. Scan the mask left-to-right, top-to-bottom.  When a non-`None` cell is
//!    found, extend it as far right as possible (same `BlockId`), then extend
//!    the resulting strip as far down as possible (every cell in the row has
//!    the same `BlockId`).  This forms the widest rectangle reachable from
//!    that starting cell.
//! 3. Emit a [`MergedQuad`] and zero-out the consumed cells in the mask.
//! 4. Repeat until the mask is exhausted.
//!
//! The caller ([`chunk_mesh`]) drives this per-direction, per-layer loop and
//! assembles the full set of quads for a chunk.

use dd40_core::block::BlockId;

use crate::face_culling::FaceDir;

// ── MergedQuad ────────────────────────────────────────────────────────────────

/// A greedy-merged quad: a maximal rectangle of same-type visible block faces
/// on a single axis-aligned slice.
///
/// Coordinates are expressed in the 2-D space of the slice:
/// - `u` is the first axis perpendicular to the face normal
/// - `v` is the second axis perpendicular to the face normal
/// - `layer` is the position along the face-normal axis
///
/// The mapping from (direction, u, v, layer) to world coordinates is performed
/// by the mesh builder.
///
/// # Axis mappings
///
/// | `dir`  | axis-normal | u-axis | v-axis |
/// |--------|-------------|--------|--------|
/// | `PosX` / `NegX` | X | Z | Y |
/// | `PosY` / `NegY` | Y | X | Z |
/// | `PosZ` / `NegZ` | Z | X | Y |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedQuad {
    /// The block type that this quad represents.
    pub block_id: BlockId,
    /// Face direction this quad belongs to.
    pub dir: FaceDir,
    /// Layer index along the face-normal axis (chunk-local).
    pub layer: usize,
    /// Start position along the first tangent axis (inclusive).
    pub u_start: usize,
    /// Start position along the second tangent axis (inclusive).
    pub v_start: usize,
    /// Extent along the first tangent axis (in blocks, ≥ 1).
    pub u_len: usize,
    /// Extent along the second tangent axis (in blocks, ≥ 1).
    pub v_len: usize,
}

// ── Public entry-point ────────────────────────────────────────────────────────

/// Runs greedy meshing on a single face direction and layer.
///
/// Given a pre-computed visibility mask (`mask[u][v]`) for a particular
/// `(dir, layer)` combination, this function merges adjacent cells with equal
/// `BlockId` into maximal axis-aligned rectangles and appends the resulting
/// [`MergedQuad`]s to `out`.
///
/// # Arguments
///
/// * `mask`  — 2-D grid of `Option<BlockId>`: `Some(id)` = face visible,
///   `None` = face hidden.  **Modified in-place** (consumed cells
///   are set to `None`).
/// * `u_len` — number of cells along the U axis (first dimension of `mask`)
/// * `v_len` — number of cells along the V axis (second dimension of `mask`)
/// * `dir`   — which of the six face directions this mask belongs to
/// * `layer` — layer index (along the face-normal axis) for the output quads
/// * `out`   — accumulated output; quads are appended here
pub fn greedy_mesh_slice(
    mask: &mut [Vec<Option<BlockId>>],
    u_len: usize,
    v_len: usize,
    dir: FaceDir,
    layer: usize,
    out: &mut Vec<MergedQuad>,
) {
    // Scan every cell in the mask.
    for u in 0..u_len {
        let mut v = 0;
        while v < v_len {
            let Some(block_id) = mask[u][v] else {
                v += 1;
                continue;
            };

            // 1. Extend width along V as far as the same BlockId continues.
            let mut width = 1;
            while v + width < v_len && mask[u][v + width] == Some(block_id) {
                width += 1;
            }

            // 2. Extend height along U as long as the entire strip [v, v+width)
            //    in row u+height has the same BlockId.
            let mut height = 1;
            'outer: while u + height < u_len {
                for dv in 0..width {
                    if mask[u + height][v + dv] != Some(block_id) {
                        break 'outer;
                    }
                }
                height += 1;
            }

            // 3. Emit the merged quad.
            out.push(MergedQuad {
                block_id,
                dir,
                layer,
                u_start: u,
                v_start: v,
                u_len: height,
                v_len: width,
            });

            // 4. Zero-out the consumed cells so they are not re-processed.
            for du in 0..height {
                for dv in 0..width {
                    mask[u + du][v + dv] = None;
                }
            }

            // Advance past the merged strip.
            v += width;
        }
    }
}

// ── Mask helpers ─────────────────────────────────────────────────────────────

/// Allocates a fresh `u_len × v_len` mask filled with `None`.
///
/// The returned `Vec<Vec<Option<BlockId>>>` is indexed as `mask[u][v]`.
pub fn empty_mask(u_len: usize, v_len: usize) -> Vec<Vec<Option<BlockId>>> {
    vec![vec![None; v_len]; u_len]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockId;

    const STONE: BlockId = BlockId(1);
    const DIRT: BlockId = BlockId(2);

    // Helper: run greedy meshing on a small mask and return the quads.
    fn run(mask: &mut Vec<Vec<Option<BlockId>>>) -> Vec<MergedQuad> {
        let u = mask.len();
        let v = if u > 0 { mask[0].len() } else { 0 };
        let mut out = Vec::new();
        greedy_mesh_slice(mask, u, v, FaceDir::PosY, 0, &mut out);
        out
    }

    // ── Empty mask ────────────────────────────────────────────────────────────

    #[test]
    fn empty_mask_produces_no_quads() {
        let mut mask = empty_mask(4, 4); // all None
        let quads = run(&mut mask);
        assert!(quads.is_empty());
    }

    // ── Single cell ───────────────────────────────────────────────────────────

    #[test]
    fn single_visible_cell_produces_one_quad() {
        let mut mask = empty_mask(4, 4);
        mask[1][2] = Some(STONE);
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert_eq!(q.block_id, STONE);
        assert_eq!(q.u_start, 1);
        assert_eq!(q.v_start, 2);
        assert_eq!(q.u_len, 1);
        assert_eq!(q.v_len, 1);
    }

    // ── Horizontal strip merging (same row, same type) ─────────────────────

    #[test]
    fn horizontal_strip_merges_into_one_quad() {
        // Row 0: [STONE, STONE, STONE, None]
        let mut mask = empty_mask(1, 4);
        mask[0][0] = Some(STONE);
        mask[0][1] = Some(STONE);
        mask[0][2] = Some(STONE);
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert_eq!(q.u_start, 0);
        assert_eq!(q.v_start, 0);
        assert_eq!(q.u_len, 1);
        assert_eq!(q.v_len, 3);
    }

    // ── Full 2×2 block of same type → one quad ───────────────────────────────

    #[test]
    fn two_by_two_same_type_is_one_quad() {
        let mut mask = empty_mask(2, 2);
        mask[0][0] = Some(STONE);
        mask[0][1] = Some(STONE);
        mask[1][0] = Some(STONE);
        mask[1][1] = Some(STONE);
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert_eq!(q.u_len, 2);
        assert_eq!(q.v_len, 2);
    }

    // ── Two different types in the same row → two quads ───────────────────────

    #[test]
    fn two_types_in_row_produce_two_quads() {
        let mut mask = empty_mask(1, 4);
        mask[0][0] = Some(STONE);
        mask[0][1] = Some(STONE);
        mask[0][2] = Some(DIRT);
        mask[0][3] = Some(DIRT);
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 2);
        // First quad is STONE, width 2.
        assert_eq!(quads[0].block_id, STONE);
        assert_eq!(quads[0].v_len, 2);
        // Second quad is DIRT, width 2.
        assert_eq!(quads[1].block_id, DIRT);
        assert_eq!(quads[1].v_len, 2);
    }

    // ── Rectangle stops expanding when types differ in the next row ───────────

    #[test]
    fn different_second_row_limits_height() {
        // Row 0: [STONE, STONE]
        // Row 1: [STONE, DIRT]
        let mut mask = empty_mask(2, 2);
        mask[0][0] = Some(STONE);
        mask[0][1] = Some(STONE);
        mask[1][0] = Some(STONE);
        mask[1][1] = Some(DIRT);

        let quads = run(&mut mask);
        // STONE strip in row 0 (width=2, height=1), then individual cells.
        let stone_2wide: Vec<&MergedQuad> = quads
            .iter()
            .filter(|q| q.block_id == STONE && q.v_len == 2)
            .collect();
        assert_eq!(stone_2wide.len(), 1, "2-wide STONE quad should exist");
        assert_eq!(stone_2wide[0].u_len, 1);
    }

    // ── Full 16×16 uniform mask → single quad ────────────────────────────────

    #[test]
    fn full_uniform_mask_produces_one_quad() {
        let mut mask = vec![vec![Some(STONE); 16]; 16];
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].u_len, 16);
        assert_eq!(quads[0].v_len, 16);
    }

    // ── Cells consumed (mask zeroed) after merging ────────────────────────────

    #[test]
    fn consumed_cells_are_cleared() {
        let mut mask = empty_mask(2, 2);
        mask[0][0] = Some(STONE);
        mask[0][1] = Some(STONE);
        mask[1][0] = Some(STONE);
        mask[1][1] = Some(STONE);
        run(&mut mask);
        // All cells should be None after the quad is consumed.
        for row in &mask {
            for cell in row {
                assert!(cell.is_none());
            }
        }
    }

    // ── Gap in strip stops the merge ─────────────────────────────────────────

    #[test]
    fn gap_in_strip_produces_two_quads() {
        // [STONE, None, STONE]
        let mut mask = empty_mask(1, 3);
        mask[0][0] = Some(STONE);
        // mask[0][1] = None
        mask[0][2] = Some(STONE);
        let quads = run(&mut mask);
        assert_eq!(quads.len(), 2);
        assert_eq!(quads[0].v_start, 0);
        assert_eq!(quads[0].v_len, 1);
        assert_eq!(quads[1].v_start, 2);
        assert_eq!(quads[1].v_len, 1);
    }

    // ── Layer and dir are threaded through correctly ───────────────────────────

    #[test]
    fn layer_and_dir_stored_in_quad() {
        let mut mask = empty_mask(1, 1);
        mask[0][0] = Some(STONE);
        let u = mask.len();
        let v = mask[0].len();
        let mut out = Vec::new();
        greedy_mesh_slice(&mut mask, u, v, FaceDir::NegZ, 7, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].dir, FaceDir::NegZ);
        assert_eq!(out[0].layer, 7);
    }

    // ── 3×3 with a hole in the middle ────────────────────────────────────────

    #[test]
    fn mask_with_hole_does_not_cover_hole() {
        // Ring of STONE around a None in the centre.
        let mut mask = empty_mask(3, 3);
        for u in 0..3 {
            for v in 0..3 {
                if !(u == 1 && v == 1) {
                    mask[u][v] = Some(STONE);
                }
            }
        }
        let quads = run(&mut mask);
        // Verify coverage: all quads together must cover 8 cells (not 9).
        let total_cells: usize = quads.iter().map(|q| q.u_len * q.v_len).sum();
        assert_eq!(total_cells, 8);
    }
}
