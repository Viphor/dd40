//! Mesh construction from greedy-merged quads.
//!
//! [`MeshBuilder`] converts a slice of [`MergedQuad`]s into a Bevy [`Mesh`]
//! suitable for use with `Mesh3d` / `MeshMaterial3d`.  Each quad produces
//! four vertices and two triangles (a triangle-list mesh).
//!
//! # Vertex attributes generated
//!
//! | Attribute              | Type          | Notes                              |
//! |------------------------|---------------|------------------------------------|
//! | `ATTRIBUTE_POSITION`   | `[f32; 3]`    | world-space corner positions       |
//! | `ATTRIBUTE_NORMAL`     | `[f32; 3]`    | constant per face direction        |
//! | `ATTRIBUTE_UV_0`       | `[f32; 2]`    | [0,1] across the quad              |
//! | `ATTRIBUTE_COLOR`      | `[f32; 4]`    | linear RGBA from `BlockDefinition` |
//!
//! # Coordinate conventions
//!
//! Each block occupies a 1×1×1 unit cube.  The chunk origin sits at
//! `(chunk_x * 16, 0, chunk_z * 16)` in world space.
//!
//! For each [`FaceDir`] the tangent axes (U, V) map as follows:
//!
//! | `dir`            | U axis | V axis | layer axis |
//! |------------------|--------|--------|------------|
//! | `PosX` / `NegX`  | Z      | Y      | X          |
//! | `PosY` / `NegY`  | X      | Z      | Y          |
//! | `PosZ` / `NegZ`  | X      | Y      | Z          |
//!
//! The layer value is the block-local coordinate along the normal axis.  For
//! `PosX` the quad sits on the **+X face** of its block, so the world-space X
//! of all four corners is `chunk_origin_x + layer + 1`.  For `NegX` it is
//! `chunk_origin_x + layer`.
//!
//! # Winding order
//!
//! All six face directions use the same triangle index pattern
//! `[0,1,2, 0,2,3]` (counter-clockwise = front face in Bevy/wgpu).
//! Correct outward normals are achieved by defining the four corner positions
//! for each direction such that, when viewed from *outside* the block, the
//! corners progress counter-clockwise.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use dd40_core::block::BlockRegistry;

use crate::{face_culling::FaceDir, greedy_mesh::MergedQuad};

// ── MeshBuilder ───────────────────────────────────────────────────────────────

/// Builds a Bevy [`Mesh`] from a collection of greedy-merged quads.
///
/// Instantiate, call [`MeshBuilder::add_quad`] (or [`MeshBuilder::add_quad_with_color`])
/// for each [`MergedQuad`], then call [`MeshBuilder::build`] to obtain the
/// finished mesh.
///
/// # Example
///
/// ```ignore
/// let mut builder = MeshBuilder::new(chunk_origin_x, chunk_origin_z);
/// for quad in &quads {
///     builder.add_quad(quad, &registry);
/// }
/// let mesh: Mesh = builder.build();
/// ```
pub struct MeshBuilder {
    /// World-space X origin of the chunk (`chunk_pos.x * 16`).
    chunk_origin_x: f32,
    /// World-space Z origin of the chunk (`chunk_pos.z * 16`).
    chunk_origin_z: f32,

    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    /// Creates a new, empty mesh builder for a chunk at the given world-space
    /// origin.
    ///
    /// # Arguments
    ///
    /// * `chunk_origin_x` — world-space X of the chunk's (0, 0, 0) corner
    /// * `chunk_origin_z` — world-space Z of the chunk's (0, 0, 0) corner
    pub fn new(chunk_origin_x: f32, chunk_origin_z: f32) -> Self {
        Self {
            chunk_origin_x,
            chunk_origin_z,
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Appends four vertices and two triangles for the given [`MergedQuad`].
    ///
    /// The quad's block color is looked up from `registry`.  If the block ID
    /// is not found in the registry the quad is silently skipped.
    ///
    /// # Arguments
    ///
    /// * `quad`     — the greedy quad to emit
    /// * `registry` — block registry used to fetch the color for the quad's
    ///   [`BlockId`]
    pub fn add_quad(&mut self, quad: &MergedQuad, registry: &BlockRegistry) {
        let color = match registry.get(quad.block_id) {
            Some(def) => linear_rgba(def.color),
            None => return,
        };
        self.add_quad_with_color(quad, color);
    }

    /// Appends four vertices and two triangles for the given [`MergedQuad`]
    /// using a pre-computed linear RGBA color.
    ///
    /// This variant bypasses the [`BlockRegistry`] lookup and is intended for
    /// use in off-thread mesh-building tasks where the registry (which is not
    /// `Send`) is not available.  Call this after pre-collecting colors into a
    /// `HashMap<BlockId, [f32; 4]>` on the main thread.
    ///
    /// # Arguments
    ///
    /// * `quad`  — the greedy quad to emit
    /// * `color` — linear RGBA `[r, g, b, a]` for the quad's block type
    pub fn add_quad_with_color(&mut self, quad: &MergedQuad, color: [f32; 4]) {
        let normal = quad.dir.normal();
        let base_index = self.positions.len() as u32;

        // Compute the four corner positions in world space.
        // Each face direction defines its corners so that, when viewed from
        // *outside* the block, they progress counter-clockwise.  This lets us
        // use a single, uniform index pattern for every face.
        let corners = quad_corners(quad, self.chunk_origin_x, self.chunk_origin_z);

        for corner in &corners {
            self.positions.push(*corner);
            self.normals.push(normal);
            self.colors.push(color);
        }

        // UV coordinates: corners go (0,0), (1,0), (1,1), (0,1).
        self.uvs.push([0.0, 0.0]);
        self.uvs.push([1.0, 0.0]);
        self.uvs.push([1.0, 1.0]);
        self.uvs.push([0.0, 1.0]);

        // Unified CCW winding — correct for every face because the corner
        // positions themselves encode the outward orientation.
        self.indices.extend_from_slice(&[
            base_index,
            base_index + 1,
            base_index + 2,
            base_index,
            base_index + 2,
            base_index + 3,
        ]);
    }

    /// Consumes the builder and returns a fully assembled Bevy [`Mesh`].
    ///
    /// Returns `None` when no quads were added (empty chunk / fully occluded).
    pub fn build(self) -> Option<Mesh> {
        if self.positions.is_empty() {
            return None;
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));

        Some(mesh)
    }
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

/// Returns the four world-space corner positions for a [`MergedQuad`].
///
/// For each face direction the corners are ordered so that, when the face is
/// viewed from *outside* the block, they progress **counter-clockwise**.
/// Combined with the uniform index pattern `[0,1,2, 0,2,3]` this guarantees
/// outward-facing normals under Bevy's default CCW front-face convention.
///
/// Axis conventions (U and V are the two tangent axes for each direction):
///
/// | dir  | normal axis | face plane coord | U axis | V axis |
/// |------|-------------|------------------|--------|--------|
/// | PosX | +X          | x = layer + 1    | Z      | Y      |
/// | NegX | −X          | x = layer        | Z      | Y      |
/// | PosY | +Y          | y = layer + 1    | X      | Z      |
/// | NegY | −Y          | y = layer        | X      | Z      |
/// | PosZ | +Z          | z = layer + 1    | X      | Y      |
/// | NegZ | −Z          | z = layer        | X      | Y      |
fn quad_corners(quad: &MergedQuad, ox: f32, oz: f32) -> [[f32; 3]; 4] {
    let layer = quad.layer as f32;
    let u0 = quad.u_start as f32;
    let v0 = quad.v_start as f32;
    let u1 = u0 + quad.u_len as f32;
    let v1 = v0 + quad.v_len as f32;

    match quad.dir {
        // ── X-axis faces ─────────────────────────────────────────────────────
        // U = Z, V = Y,  normal axis = X
        //
        // PosX: face at x = layer + 1.
        // Viewed from +X looking in −X: CCW order is bottom-right → bottom-left
        // → top-left → top-right (right-hand rule with normal pointing toward
        // the viewer).
        FaceDir::PosX => {
            let x = ox + layer + 1.0;
            [
                [x, v0, oz + u1], // bottom-right
                [x, v0, oz + u0], // bottom-left
                [x, v1, oz + u0], // top-left
                [x, v1, oz + u1], // top-right
            ]
        }
        // NegX: face at x = layer.
        // Viewed from −X looking in +X: CCW order is bottom-left → bottom-right
        // → top-right → top-left.
        FaceDir::NegX => {
            let x = ox + layer;
            [
                [x, v0, oz + u0], // bottom-left
                [x, v0, oz + u1], // bottom-right
                [x, v1, oz + u1], // top-right
                [x, v1, oz + u0], // top-left
            ]
        }

        // ── Y-axis faces ─────────────────────────────────────────────────────
        // U = X, V = Z,  normal axis = Y
        //
        // PosY: face at y = layer + 1 (top of block).
        // Viewed from +Y looking down (−Y): CCW in the XZ plane.
        FaceDir::PosY => {
            let y = layer + 1.0;
            [
                [ox + u0, y, oz + v1], // front-left
                [ox + u1, y, oz + v1], // front-right
                [ox + u1, y, oz + v0], // back-right
                [ox + u0, y, oz + v0], // back-left
            ]
        }
        // NegY: face at y = layer (bottom of block).
        // Viewed from −Y looking up (+Y): CCW in the XZ plane (reversed Z).
        FaceDir::NegY => {
            let y = layer;
            [
                [ox + u0, y, oz + v0], // back-left
                [ox + u1, y, oz + v0], // back-right
                [ox + u1, y, oz + v1], // front-right
                [ox + u0, y, oz + v1], // front-left
            ]
        }

        // ── Z-axis faces ─────────────────────────────────────────────────────
        // U = X, V = Y,  normal axis = Z
        //
        // PosZ: face at z = layer + 1.
        // Viewed from +Z looking in −Z: CCW order is bottom-left → bottom-right
        // → top-right → top-left.
        // Cross product check: e1=[1,0,0], e2=[1,1,0] → cross=[0,0,1] = +Z ✓
        FaceDir::PosZ => {
            let z = oz + layer + 1.0;
            [
                [ox + u0, v0, z], // bottom-left
                [ox + u1, v0, z], // bottom-right
                [ox + u1, v1, z], // top-right
                [ox + u0, v1, z], // top-left
            ]
        }
        // NegZ: face at z = layer.
        // Viewed from −Z looking in +Z: CCW order is bottom-right → bottom-left
        // → top-left → top-right.
        // Cross product check: e1=[-1,0,0], e2=[-1,1,0] → cross=[0,0,-1] = -Z ✓
        FaceDir::NegZ => {
            let z = oz + layer;
            [
                [ox + u1, v0, z], // bottom-right
                [ox + u0, v0, z], // bottom-left
                [ox + u0, v1, z], // top-left
                [ox + u1, v1, z], // top-right
            ]
        }
    }
}

/// Converts a Bevy [`Color`] to a linear RGBA `[f32; 4]` array.
pub(crate) fn linear_rgba(color: Color) -> [f32; 4] {
    let c = color.to_linear();
    [c.red, c.green, c.blue, c.alpha]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::block::BlockId;

    const STONE: BlockId = BlockId(1);

    fn stone_registry() -> BlockRegistry {
        // BlockRegistry::new() gives us air at ID 0 only.
        // We cannot call register() without Commands, so we test with a
        // registry that has only air and validate the "block not found →
        // skip" path, plus geometry helpers independently.
        BlockRegistry::new()
    }

    // ── Empty builder ─────────────────────────────────────────────────────────

    #[test]
    fn empty_builder_returns_none() {
        let builder = MeshBuilder::new(0.0, 0.0);
        assert!(builder.build().is_none());
    }

    // ── Geometry helpers ──────────────────────────────────────────────────────

    /// A unit quad at layer 0, u_start=0, v_start=0, u_len=1, v_len=1.
    fn unit_quad(dir: FaceDir) -> MergedQuad {
        MergedQuad {
            block_id: STONE,
            dir,
            layer: 0,
            u_start: 0,
            v_start: 0,
            u_len: 1,
            v_len: 1,
        }
    }

    #[test]
    fn pos_y_face_at_layer_0_has_y_equal_1() {
        let q = unit_quad(FaceDir::PosY);
        let corners = quad_corners(&q, 0.0, 0.0);
        for c in &corners {
            assert!(
                (c[1] - 1.0).abs() < 1e-6,
                "+Y face at layer 0 should be at y=1, got {}",
                c[1]
            );
        }
    }

    #[test]
    fn neg_y_face_at_layer_0_has_y_equal_0() {
        let q = unit_quad(FaceDir::NegY);
        let corners = quad_corners(&q, 0.0, 0.0);
        for c in &corners {
            assert!(
                c[1].abs() < 1e-6,
                "-Y face at layer 0 should be at y=0, got {}",
                c[1]
            );
        }
    }

    #[test]
    fn pos_x_face_at_layer_2_has_x_equal_chunk_origin_plus_3() {
        // chunk origin at x=16 (chunk 1), layer 2 → x = 16 + 2 + 1 = 19
        let mut q = unit_quad(FaceDir::PosX);
        q.layer = 2;
        let corners = quad_corners(&q, 16.0, 0.0);
        for c in &corners {
            assert!(
                (c[0] - 19.0).abs() < 1e-6,
                "+X face expected x=19, got {}",
                c[0]
            );
        }
    }

    #[test]
    fn neg_x_face_at_layer_3_has_x_equal_chunk_origin_plus_3() {
        // chunk origin at x=0, layer 3 → x = 0 + 3 = 3
        let mut q = unit_quad(FaceDir::NegX);
        q.layer = 3;
        let corners = quad_corners(&q, 0.0, 0.0);
        for c in &corners {
            assert!(
                (c[0] - 3.0).abs() < 1e-6,
                "-X face expected x=3, got {}",
                c[0]
            );
        }
    }

    #[test]
    fn quad_with_u_len_3_v_len_2_spans_correct_range() {
        // +Y face, u_len=3 (X extent), v_len=2 (Z extent)
        let q = MergedQuad {
            block_id: STONE,
            dir: FaceDir::PosY,
            layer: 0,
            u_start: 1,
            v_start: 4,
            u_len: 3,
            v_len: 2,
        };
        let corners = quad_corners(&q, 0.0, 0.0);
        // X range should be [1, 4], Z range should be [4, 6].
        let xs: Vec<f32> = corners.iter().map(|c| c[0]).collect();
        let zs: Vec<f32> = corners.iter().map(|c| c[2]).collect();
        assert!(xs.iter().any(|&x| (x - 1.0).abs() < 1e-6));
        assert!(xs.iter().any(|&x| (x - 4.0).abs() < 1e-6));
        assert!(zs.iter().any(|&z| (z - 4.0).abs() < 1e-6));
        assert!(zs.iter().any(|&z| (z - 6.0).abs() < 1e-6));
    }

    // ── Winding order: all faces use uniform CCW indices ──────────────────────

    /// Verify that for every face direction the cross product of the first two
    /// triangle edges points in the same direction as the declared normal.
    /// This confirms CCW winding when viewed from outside the block.
    #[test]
    fn all_faces_have_outward_winding() {
        for dir in FaceDir::ALL {
            let q = unit_quad(dir);
            let corners = quad_corners(&q, 0.0, 0.0);

            // Triangle 0: corners[0], corners[1], corners[2]
            let v0 = corners[0];
            let v1 = corners[1];
            let v2 = corners[2];

            // Edge vectors
            let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

            // Cross product e1 × e2 gives the geometric normal.
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            let expected = dir.normal();

            // The dot product of the cross product with the expected normal
            // must be positive (same half-space).
            let dot = cross[0] * expected[0] + cross[1] * expected[1] + cross[2] * expected[2];
            assert!(
                dot > 0.0,
                "Face {:?}: geometric normal points inward (dot = {}). \
                 Corner winding is wrong.",
                dir,
                dot
            );
        }
    }

    // ── PosX and NegX produce different corner orders ─────────────────────────

    /// PosX and NegX share the same wall-x formula (apart from +1 offset) and
    /// the same u/v extents, but their corner sequences must differ so the
    /// winding faces outward in opposite directions.
    ///
    /// We verify this by checking that the two Z-sequences are not identical
    /// (they are distinct orderings of the same two values).
    #[test]
    fn pos_x_and_neg_x_corners_differ() {
        let pos_q = unit_quad(FaceDir::PosX);
        let neg_q = unit_quad(FaceDir::NegX);
        let pos_corners = quad_corners(&pos_q, 0.0, 0.0);
        let neg_corners = quad_corners(&neg_q, 0.0, 0.0);

        let pos_zs: Vec<f32> = pos_corners.iter().map(|c| c[2]).collect();
        let neg_zs: Vec<f32> = neg_corners.iter().map(|c| c[2]).collect();

        // PosX: [u1, u0, u0, u1] and NegX: [u0, u1, u1, u0] — different orders.
        assert_ne!(
            pos_zs, neg_zs,
            "PosX and NegX should have opposite corner Z sequences"
        );

        // Both sequences must contain both u0=0.0 and u1=1.0.
        assert!(pos_zs.contains(&0.0) && pos_zs.contains(&1.0));
        assert!(neg_zs.contains(&0.0) && neg_zs.contains(&1.0));

        // The first Z of PosX (u1) must differ from the first Z of NegX (u0).
        assert_ne!(
            pos_corners[0][2], neg_corners[0][2],
            "PosX and NegX should start at opposite Z corners"
        );
    }

    // ── linear_rgba ───────────────────────────────────────────────────────────

    #[test]
    fn linear_rgba_white() {
        let c = linear_rgba(Color::WHITE);
        assert!((c[0] - 1.0).abs() < 1e-5);
        assert!((c[1] - 1.0).abs() < 1e-5);
        assert!((c[2] - 1.0).abs() < 1e-5);
        assert!((c[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn linear_rgba_black() {
        let c = linear_rgba(Color::BLACK);
        assert!(c[0].abs() < 1e-5);
        assert!(c[1].abs() < 1e-5);
        assert!(c[2].abs() < 1e-5);
        assert!((c[3] - 1.0).abs() < 1e-5);
    }

    // ── unknown block id is skipped ───────────────────────────────────────────

    #[test]
    fn unknown_block_id_skips_quad() {
        let registry = stone_registry(); // only has air (ID 0)
        let mut builder = MeshBuilder::new(0.0, 0.0);
        let q = unit_quad(FaceDir::PosY); // STONE = BlockId(1), not in registry
        builder.add_quad(&q, &registry);
        // No vertices added → build returns None.
        assert!(builder.build().is_none());
    }

    // ── add_quad_with_color bypasses registry ─────────────────────────────────

    #[test]
    fn add_quad_with_color_produces_mesh_without_registry() {
        let mut builder = MeshBuilder::new(0.0, 0.0);
        let q = unit_quad(FaceDir::PosY);
        // Provide color directly — no registry needed.
        builder.add_quad_with_color(&q, [1.0, 0.0, 0.0, 1.0]);
        // Should produce a valid mesh.
        assert!(builder.build().is_some());
    }

    // ── PosZ and NegZ winding ─────────────────────────────────────────────────

    /// PosZ and NegZ have opposite windings.  We verify the X-sequences differ
    /// and that each starts at the opposite end (mirrored first corner).
    #[test]
    fn pos_z_and_neg_z_corners_differ() {
        let pos_q = unit_quad(FaceDir::PosZ);
        let neg_q = unit_quad(FaceDir::NegZ);
        let pos_corners = quad_corners(&pos_q, 0.0, 0.0);
        let neg_corners = quad_corners(&neg_q, 0.0, 0.0);

        // PosZ: [u0, u1, u1, u0] and NegZ: [u1, u0, u0, u1] — different orders.
        let pos_xs: Vec<f32> = pos_corners.iter().map(|c| c[0]).collect();
        let neg_xs: Vec<f32> = neg_corners.iter().map(|c| c[0]).collect();

        assert_ne!(
            pos_xs, neg_xs,
            "PosZ and NegZ should have opposite corner X sequences"
        );

        // Both contain u0=0.0 and u1=1.0.
        assert!(pos_xs.contains(&0.0) && pos_xs.contains(&1.0));
        assert!(neg_xs.contains(&0.0) && neg_xs.contains(&1.0));

        // The first X of PosZ (u0) must differ from the first X of NegZ (u1).
        assert_ne!(
            pos_corners[0][0], neg_corners[0][0],
            "PosZ and NegZ should start at opposite X corners"
        );
    }
}
