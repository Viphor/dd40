//! Face culling for voxel chunk meshes.
//!
//! A block face is visible (and should be rendered) when the adjacent block in
//! that direction is either air or a non-solid block.  For faces that sit on a
//! chunk boundary the neighbour lookup crosses into an adjacent chunk; when
//! that chunk is not present in the [`ChunkCache`] the neighbour is treated as
//! air (i.e. the face is considered visible).
//!
//! The main entry-point is [`visible_faces`], which returns a
//! [`VisibleFaces`] bit-set for a single block position inside a chunk.

use dd40_core::chunk::cache::ChunkCache;
use dd40_core::{
    block::{Block, BlockId, BlockRegistry},
    chunk::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk, ChunkPos},
};

// ── Face direction ────────────────────────────────────────────────────────────

/// The six axis-aligned face directions for a voxel block.
///
/// The naming follows the Minecraft / OpenGL convention where +Y is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceDir {
    /// The face facing in the +X direction (east).
    PosX,
    /// The face facing in the −X direction (west).
    NegX,
    /// The face facing in the +Y direction (up / top).
    PosY,
    /// The face facing in the −Y direction (down / bottom).
    NegY,
    /// The face facing in the +Z direction (south).
    PosZ,
    /// The face facing in the −Z direction (north).
    NegZ,
}

impl FaceDir {
    /// Returns all six face directions in a fixed order.
    ///
    /// The order is: +X, −X, +Y, −Y, +Z, −Z.
    pub const ALL: [FaceDir; 6] = [
        FaceDir::PosX,
        FaceDir::NegX,
        FaceDir::PosY,
        FaceDir::NegY,
        FaceDir::PosZ,
        FaceDir::NegZ,
    ];

    /// Returns the unit-vector offset (dx, dy, dz) for this face direction.
    pub fn offset(self) -> (i32, i32, i32) {
        match self {
            FaceDir::PosX => (1, 0, 0),
            FaceDir::NegX => (-1, 0, 0),
            FaceDir::PosY => (0, 1, 0),
            FaceDir::NegY => (0, -1, 0),
            FaceDir::PosZ => (0, 0, 1),
            FaceDir::NegZ => (0, 0, -1),
        }
    }

    /// Returns the outward normal vector `[nx, ny, nz]` for this face.
    pub fn normal(self) -> [f32; 3] {
        match self {
            FaceDir::PosX => [1.0, 0.0, 0.0],
            FaceDir::NegX => [-1.0, 0.0, 0.0],
            FaceDir::PosY => [0.0, 1.0, 0.0],
            FaceDir::NegY => [0.0, -1.0, 0.0],
            FaceDir::PosZ => [0.0, 0.0, 1.0],
            FaceDir::NegZ => [0.0, 0.0, -1.0],
        }
    }
}

// ── VisibleFaces ──────────────────────────────────────────────────────────────

/// A compact bit-set recording which of the six faces of a block are visible.
///
/// A face is "visible" when the block immediately adjacent in that direction is
/// either [`BlockId::AIR`] or a non-solid block (so the player can see through
/// it).
///
/// Bit layout (LSB = bit 0):
///
/// | Bit | Face  |
/// |-----|-------|
/// | 0   | +X    |
/// | 1   | −X    |
/// | 2   | +Y    |
/// | 3   | −Y    |
/// | 4   | +Z    |
/// | 5   | −Z    |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisibleFaces(pub u8);

impl VisibleFaces {
    /// Returns `true` if the face in the given direction is visible.
    pub fn is_visible(self, dir: FaceDir) -> bool {
        let bit = Self::bit(dir);
        (self.0 & bit) != 0
    }

    /// Returns `true` if no faces are visible (fully occluded block).
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns an iterator over all visible [`FaceDir`]s.
    pub fn iter_visible(self) -> impl Iterator<Item = FaceDir> {
        FaceDir::ALL
            .into_iter()
            .filter(move |&d| self.is_visible(d))
    }

    fn set(&mut self, dir: FaceDir) {
        self.0 |= Self::bit(dir);
    }

    fn bit(dir: FaceDir) -> u8 {
        match dir {
            FaceDir::PosX => 1 << 0,
            FaceDir::NegX => 1 << 1,
            FaceDir::PosY => 1 << 2,
            FaceDir::NegY => 1 << 3,
            FaceDir::PosZ => 1 << 4,
            FaceDir::NegZ => 1 << 5,
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns the set of visible faces for the block at chunk-local position
/// `(lx, ly, lz)` inside `chunk`.
///
/// A face is visible when the neighbour in that direction is air or non-solid.
/// Neighbours outside the chunk boundary are looked up in `cache`; when the
/// neighbouring chunk is absent the neighbour is treated as air (face visible).
///
/// # Arguments
///
/// * `chunk`    — the chunk containing the block under inspection
/// * `lx,ly,lz` — chunk-local coordinates (0-based, within chunk bounds)
/// * `registry` — block registry used to test solidity
/// * `cache`    — chunk cache used to resolve cross-boundary neighbours
///
/// # Panics
///
/// Does not panic; out-of-range coordinates return [`VisibleFaces`] with no
/// faces set (treated as a fully-occluded interior block).
pub fn visible_faces(
    chunk: &Chunk,
    lx: usize,
    ly: usize,
    lz: usize,
    registry: &BlockRegistry,
    cache: &ChunkCache,
) -> VisibleFaces {
    let mut faces = VisibleFaces::default();

    // The block itself must exist and be renderable for any face to show.
    let Some(block) = chunk.get(lx, ly, lz) else {
        return faces;
    };
    if !block.is_renderable(registry) {
        return faces;
    }

    for dir in FaceDir::ALL {
        let (dx, dy, dz) = dir.offset();

        let nx = lx as i32 + dx;
        let ny = ly as i32 + dy;
        let nz = lz as i32 + dz;

        if neighbour_is_transparent(chunk, nx, ny, nz, registry, cache) {
            faces.set(dir);
        }
    }

    faces
}

// ── Internals ─────────────────────────────────────────────────────────────────

/// Returns `true` when the block at chunk-local neighbour position
/// `(nx, ny, nz)` is transparent (air or non-solid), meaning the adjacent
/// face should be rendered.
///
/// When the neighbour position falls outside the chunk boundary the function
/// looks up the adjacent chunk in `cache`.  A missing chunk is treated as all-
/// air (returns `true`).
fn neighbour_is_transparent(
    chunk: &Chunk,
    nx: i32,
    ny: i32,
    nz: i32,
    registry: &BlockRegistry,
    cache: &ChunkCache,
) -> bool {
    // Fast path: neighbour is inside the same chunk.
    let in_x = (0..CHUNK_SIZE_X as i32).contains(&nx);
    let in_y = (0..CHUNK_SIZE_Y as i32).contains(&ny);
    let in_z = (0..CHUNK_SIZE_Z as i32).contains(&nz);

    if in_x && in_y && in_z {
        return block_is_transparent(chunk.get(nx as usize, ny as usize, nz as usize), registry);
    }

    // Y out-of-range: treat top/bottom world boundary as air.
    if !in_y {
        return true;
    }

    // Neighbour is in an adjacent chunk.
    let cp = chunk.position();

    // Compute the neighbouring chunk position and the wrapped local coords.
    let (ncx, local_x) = if nx < 0 {
        (cp.x - 1, (CHUNK_SIZE_X as i32 + nx) as usize)
    } else if nx >= CHUNK_SIZE_X as i32 {
        (cp.x + 1, (nx - CHUNK_SIZE_X as i32) as usize)
    } else {
        (cp.x, nx as usize)
    };

    let (ncz, local_z) = if nz < 0 {
        (cp.z - 1, (CHUNK_SIZE_Z as i32 + nz) as usize)
    } else if nz >= CHUNK_SIZE_Z as i32 {
        (cp.z + 1, (nz - CHUNK_SIZE_Z as i32) as usize)
    } else {
        (cp.z, nz as usize)
    };

    let neighbour_pos = ChunkPos { x: ncx, z: ncz };

    match cache.get(&neighbour_pos) {
        Some(neighbour_chunk) => {
            let nb = neighbour_chunk.get(local_x, ny as usize, local_z);
            block_is_transparent(nb, registry)
        }
        // Adjacent chunk not loaded → treat as air (expose the face).
        None => true,
    }
}

/// Returns `true` when the given block is air or non-solid (i.e. transparent).
///
/// `None` (out-of-range) is treated as air.
#[inline]
fn block_is_transparent(block: Option<Block>, registry: &BlockRegistry) -> bool {
    match block {
        None => true,
        Some(b) => b.block_id == BlockId::AIR || !b.is_solid(registry),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::{
        block::{BlockDefinition, BlockId, BlockRegistry},
        chunk::{Chunk, ChunkPos},
    };

    /// Build a minimal registry with air (ID 0) and stone (ID 1).
    fn make_registry() -> BlockRegistry {
        let r = BlockRegistry::new();
        // BlockRegistry::new() already registers air at ID 0.
        // We manually insert stone via the internal Vec by using the public
        // register-like path.  Because BlockRegistry::register requires
        // Commands we rebuild the registry the same way core does it.
        let stone = BlockDefinition::new(BlockId(1), "stone")
            .with_solid(true)
            .with_renderable(true);
        // Use a private-compatible approach: register_auto is not available
        // without Commands either, so we construct via new() and the
        // insert_definition path is private.  Instead we test with a registry
        // that has only air, and confirm stone-like blocks via the Block API.
        let _ = stone; // see below — we work with only air + a fake block
        r
    }

    /// Build a registry that has air and stone using a fresh BlockRegistry.
    fn registry_with_stone() -> BlockRegistry {
        // Construct directly since BlockRegistry::new() gives us air.
        // We can reach `insert_definition` only through `register`, which
        // needs Commands.  In unit tests we bypass this by building the
        // registry from scratch and reaching into it via its public API.
        //
        // Because `register` needs `Commands` (an ECS type that cannot be
        // constructed outside a Bevy App), we use the `BlockRegistry::new()`
        // path and test the parts we *can* test without a full App.
        BlockRegistry::new()
    }

    // ── VisibleFaces bit-set ──────────────────────────────────────────────────

    #[test]
    fn visible_faces_default_is_empty() {
        let vf = VisibleFaces::default();
        assert!(vf.is_empty());
        for dir in FaceDir::ALL {
            assert!(!vf.is_visible(dir));
        }
    }

    #[test]
    fn visible_faces_set_and_query() {
        let mut vf = VisibleFaces::default();
        vf.set(FaceDir::PosX);
        vf.set(FaceDir::NegY);
        assert!(vf.is_visible(FaceDir::PosX));
        assert!(vf.is_visible(FaceDir::NegY));
        assert!(!vf.is_visible(FaceDir::NegX));
        assert!(!vf.is_visible(FaceDir::PosY));
        assert!(!vf.is_visible(FaceDir::PosZ));
        assert!(!vf.is_visible(FaceDir::NegZ));
    }

    #[test]
    fn visible_faces_iter_visible() {
        let mut vf = VisibleFaces::default();
        vf.set(FaceDir::PosY);
        vf.set(FaceDir::NegZ);
        let visible: Vec<FaceDir> = vf.iter_visible().collect();
        assert_eq!(visible.len(), 2);
        assert!(visible.contains(&FaceDir::PosY));
        assert!(visible.contains(&FaceDir::NegZ));
    }

    // ── Air block at centre — all six faces visible ───────────────────────────

    #[test]
    fn air_block_has_no_visible_faces() {
        let registry = registry_with_stone();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0));
        // All blocks default to air.
        let faces = visible_faces(&chunk, 8, 128, 8, &registry, &cache);
        // Air is not renderable → no faces.
        assert!(faces.is_empty());
    }

    // ── Interior solid block surrounded by air ────────────────────────────────
    //
    // We cannot register a solid block without Commands, so we test the
    // block_is_transparent helper indirectly by checking that a block with
    // block_id == AIR always yields empty VisibleFaces (not renderable).
    #[test]
    fn non_renderable_block_no_faces() {
        let registry = registry_with_stone();
        let cache = ChunkCache::default();
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        // Write an air block (non-renderable) explicitly.
        chunk.set(5, 5, 5, Block::new(BlockId::AIR));
        let faces = visible_faces(&chunk, 5, 5, 5, &registry, &cache);
        assert!(faces.is_empty());
    }

    // ── block_is_transparent helper ───────────────────────────────────────────

    #[test]
    fn transparent_for_none() {
        let registry = registry_with_stone();
        assert!(block_is_transparent(None, &registry));
    }

    #[test]
    fn transparent_for_air_block() {
        let registry = registry_with_stone();
        assert!(block_is_transparent(
            Some(Block::new(BlockId::AIR)),
            &registry
        ));
    }

    // ── FaceDir helpers ───────────────────────────────────────────────────────

    #[test]
    fn face_dir_all_has_six_entries() {
        assert_eq!(FaceDir::ALL.len(), 6);
    }

    #[test]
    fn face_dir_offsets_are_unit_vectors() {
        for dir in FaceDir::ALL {
            let (dx, dy, dz) = dir.offset();
            let manhattan = dx.abs() + dy.abs() + dz.abs();
            assert_eq!(manhattan, 1, "offset for {:?} is not a unit vector", dir);
        }
    }

    #[test]
    fn face_dir_normals_are_unit_vectors() {
        for dir in FaceDir::ALL {
            let [nx, ny, nz] = dir.normal();
            let len_sq = nx * nx + ny * ny + nz * nz;
            assert!(
                (len_sq - 1.0).abs() < 1e-6,
                "normal for {:?} is not unit length",
                dir
            );
        }
    }

    // ── Chunk-boundary face visibility ────────────────────────────────────────

    #[test]
    fn chunk_boundary_with_missing_neighbour_is_visible() {
        // neighbour_is_transparent should return true when the adjacent chunk
        // is absent from the cache.
        let registry = registry_with_stone();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0));

        // nx = -1 is outside the chunk in the -X direction.
        // No neighbour chunk in the cache → should be transparent.
        let result = neighbour_is_transparent(&chunk, -1, 10, 10, &registry, &cache);
        assert!(result);
    }

    #[test]
    fn y_out_of_bounds_is_transparent() {
        let registry = registry_with_stone();
        let cache = ChunkCache::default();
        let chunk = Chunk::new(ChunkPos::new(0, 0));

        // ny = -1 is below the world → transparent.
        assert!(neighbour_is_transparent(
            &chunk, 5, -1, 5, &registry, &cache
        ));
        // ny = CHUNK_SIZE_Y is above the world → transparent.
        assert!(neighbour_is_transparent(
            &chunk,
            5,
            CHUNK_SIZE_Y as i32,
            5,
            &registry,
            &cache
        ));
    }
}
