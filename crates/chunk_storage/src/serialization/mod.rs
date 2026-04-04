//! Binary serialization and deserialization for [`Chunk`].
//!
//! # File format
//!
//! All multi-byte integers are stored in **little-endian** byte order.
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │  Header (14 bytes)                                 │
//! │    magic:   [u8; 4]  = 0x44 0x44 0x34 0x30         │
//! │    version: u16                                    │
//! │    chunk_x: i32                                    │
//! │    chunk_z: i32                                    │
//! ├────────────────────────────────────────────────────┤
//! │  Body (variable, format depends on version)        │
//! │    v1 — RLE-encoded block IDs (see v1.rs)          │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! # Versioning
//!
//! The version field in the header determines which body codec is used.
//! [`deserialize_chunk`] reads the header first, then dispatches to the
//! appropriate version module — so older files remain readable even after the
//! format is updated.
//!
//! To add a new version:
//! 1. Create `vN.rs` implementing `serialize_body` and `deserialize_body`.
//! 2. Add a `VN` variant to [`ChunkVersion`].
//! 3. Add an arm to the `match` blocks in [`serialize_chunk_versioned`] and
//!    [`deserialize_chunk`].
//! 4. Update [`LATEST_VERSION`].

use std::io::{self, Read, Write};

use dd40_core::prelude::*;

pub mod v1;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Four-byte magic number at the start of every chunk file: ASCII "DD40".
pub const MAGIC: [u8; 4] = [0x44, 0x44, 0x34, 0x30];

/// The version that [`serialize_chunk`] writes. Bump this when adding a new
/// version module and update the match arms accordingly.
const LATEST_VERSION: ChunkVersion = ChunkVersion::V1;

// ---------------------------------------------------------------------------
// ChunkVersion
// ---------------------------------------------------------------------------

/// Identifies the body codec used in a chunk file.
///
/// Each variant maps 1-to-1 to a version submodule (`v1`, `v2`, …) and to the
/// integer stored in the file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkVersion {
    /// Version 1 — run-length encoded `(run_len: u16, block_id: u16)` pairs.
    V1 = 1,
}

impl ChunkVersion {
    /// Returns the integer representation written into the file header.
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    /// Converts the raw integer from a file header back to a [`ChunkVersion`],
    /// or returns [`ChunkSerializeError::UnsupportedVersion`] if the value is
    /// not recognised.
    fn from_u16(v: u16) -> Result<Self, ChunkSerializeError> {
        match v {
            1 => Ok(Self::V1),
            other => Err(ChunkSerializeError::UnsupportedVersion(other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during chunk serialization or deserialization.
#[derive(Debug)]
pub enum ChunkSerializeError {
    /// Underlying I/O failure.
    Io(io::Error),
    /// The byte stream does not start with the expected magic bytes.
    BadMagic([u8; 4]),
    /// The version field in the header is not supported by this implementation.
    UnsupportedVersion(u16),
    /// The body codec decoded a number of blocks other than [`CHUNK_SIZE`].
    ///
    /// [`CHUNK_SIZE`]: dd40_core::chunk::CHUNK_SIZE
    UnexpectedBlockCount { expected: usize, actual: usize },
}

impl std::fmt::Display for ChunkSerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "chunk I/O error: {e}"),
            Self::BadMagic(got) => {
                write!(f, "bad magic bytes: expected {:?}, got {:?}", MAGIC, got)
            }
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported chunk format version: {v}")
            }
            Self::UnexpectedBlockCount { expected, actual } => {
                write!(f, "body decoded {actual} blocks, expected {expected}")
            }
        }
    }
}

impl std::error::Error for ChunkSerializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ChunkSerializeError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Serializes `chunk` into `writer` using the **latest** file format version.
///
/// The output is fully self-contained — the chunk position and version are
/// embedded in the header.
///
/// # Errors
///
/// Returns [`ChunkSerializeError::Io`] if any write to `writer` fails.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use std::io::BufWriter;
/// use dd40_core::prelude::*;
/// use dd40_chunk_storage::serialization::serialize_chunk;
///
/// let chunk = Chunk::new(ChunkPos::new(0, 0));
/// let file = File::create("chunk_0_0.bin").unwrap();
/// serialize_chunk(&chunk, BufWriter::new(file)).unwrap();
/// ```
pub fn serialize_chunk<W: Write>(chunk: &Chunk, writer: W) -> Result<(), ChunkSerializeError> {
    serialize_chunk_versioned(chunk, writer, LATEST_VERSION)
}

/// Serializes `chunk` into `writer` using the **specified** file format version.
///
/// Prefer [`serialize_chunk`] for normal use. This function exists so that
/// tooling (e.g. migration utilities or tests) can write a specific older
/// version for compatibility verification.
///
/// # Errors
///
/// Returns [`ChunkSerializeError::Io`] if any write to `writer` fails.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use std::io::BufWriter;
/// use dd40_core::prelude::*;
/// use dd40_chunk_storage::serialization::{ChunkVersion, serialize_chunk_versioned};
///
/// let chunk = Chunk::new(ChunkPos::new(0, 0));
/// let file = File::create("chunk_0_0_v1.bin").unwrap();
/// serialize_chunk_versioned(&chunk, BufWriter::new(file), ChunkVersion::V1).unwrap();
/// ```
pub fn serialize_chunk_versioned<W: Write>(
    chunk: &Chunk,
    mut writer: W,
    version: ChunkVersion,
) -> Result<(), ChunkSerializeError> {
    let pos = chunk.position();

    // ---- Shared header ----
    writer.write_all(&MAGIC)?;
    writer.write_all(&version.as_u16().to_le_bytes())?;
    writer.write_all(&pos.x.to_le_bytes())?;
    writer.write_all(&pos.z.to_le_bytes())?;

    // ---- Version-specific body ----
    match version {
        ChunkVersion::V1 => v1::serialize_body(chunk, &mut writer)?,
    }

    Ok(())
}

/// Deserializes a [`Chunk`] from `reader`.
///
/// Reads the shared header first, then dispatches to the body codec for the
/// version stored in the header. This means files written by any previously
/// supported version can still be read.
///
/// # Errors
///
/// - [`ChunkSerializeError::Io`] — any read failure, including unexpected EOF.
/// - [`ChunkSerializeError::BadMagic`] — the stream does not begin with the
///   expected magic bytes.
/// - [`ChunkSerializeError::UnsupportedVersion`] — the version in the header
///   is not handled by this build.
/// - [`ChunkSerializeError::UnexpectedBlockCount`] — the body decoded a wrong
///   number of blocks.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use std::io::BufReader;
/// use dd40_chunk_storage::serialization::deserialize_chunk;
///
/// let file = File::open("chunk_0_0.bin").unwrap();
/// let chunk = deserialize_chunk(BufReader::new(file)).unwrap();
/// ```
pub fn deserialize_chunk<R: Read>(mut reader: R) -> Result<Chunk, ChunkSerializeError> {
    // ---- Shared header ----
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(ChunkSerializeError::BadMagic(magic));
    }

    let version = ChunkVersion::from_u16(read_u16(&mut reader)?)?;

    let chunk_x = read_i32(&mut reader)?;
    let chunk_z = read_i32(&mut reader)?;
    let pos = ChunkPos::new(chunk_x, chunk_z);

    // ---- Version-specific body ----
    match version {
        ChunkVersion::V1 => v1::deserialize_body(pos, &mut reader),
    }
}

// ---------------------------------------------------------------------------
// Chunk extension trait
// ---------------------------------------------------------------------------

/// Adds [`save`](ChunkSerializeExt::save) and [`load`](ChunkSerializeExt::load)
/// convenience methods to [`Chunk`].
pub trait ChunkSerializeExt: Sized {
    /// Writes the chunk to `path` using the latest file format.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Errors
    ///
    /// Propagates any [`ChunkSerializeError`] from [`serialize_chunk`] or from
    /// directory / file creation.
    fn save(&self, path: &std::path::Path) -> Result<(), ChunkSerializeError>;

    /// Reads a chunk from `path`, auto-detecting the version from the header.
    ///
    /// # Errors
    ///
    /// Propagates any [`ChunkSerializeError`] from [`deserialize_chunk`].
    fn load(path: &std::path::Path) -> Result<Self, ChunkSerializeError>;
}

impl ChunkSerializeExt for Chunk {
    fn save(&self, path: &std::path::Path) -> Result<(), ChunkSerializeError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        serialize_chunk(self, std::io::BufWriter::new(file))
    }

    fn load(path: &std::path::Path) -> Result<Self, ChunkSerializeError> {
        let file = std::fs::File::open(path)?;
        deserialize_chunk(std::io::BufReader::new(file))
    }
}

// ---------------------------------------------------------------------------
// Shared helpers (used by version submodules)
// ---------------------------------------------------------------------------

/// Converts a flat storage index to chunk-local `(lx, ly, lz)`.
///
/// This is the inverse of the index formula used throughout the codebase:
/// `index = lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z`
#[inline(always)]
pub(super) fn flat_to_local(index: usize) -> (usize, usize, usize) {
    let lx = index % CHUNK_SIZE_X;
    let remainder = index / CHUNK_SIZE_X;
    let lz = remainder % CHUNK_SIZE_Z;
    let ly = remainder / CHUNK_SIZE_Z;
    (lx, ly, lz)
}

#[inline(always)]
fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

#[inline(always)]
fn read_i32<R: Read>(reader: &mut R) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use dd40_core::prelude::*;

    use super::*;

    // Helper: round-trip a chunk through an in-memory buffer using the latest
    // version and return the deserialized copy.
    fn round_trip(chunk: &Chunk) -> Chunk {
        let mut buf = Vec::new();
        serialize_chunk(chunk, &mut buf).expect("serialize failed");
        deserialize_chunk(buf.as_slice()).expect("deserialize failed")
    }

    // Helper: round-trip using an explicit version.
    fn round_trip_versioned(chunk: &Chunk, version: ChunkVersion) -> Chunk {
        let mut buf = Vec::new();
        serialize_chunk_versioned(chunk, &mut buf, version).expect("serialize_versioned failed");
        deserialize_chunk(buf.as_slice()).expect("deserialize failed")
    }

    // Helper: collect every block in storage order.
    fn all_blocks(chunk: &Chunk) -> Vec<Block> {
        let mut blocks = Vec::with_capacity(CHUNK_SIZE);
        for i in 0..CHUNK_SIZE {
            let (lx, ly, lz) = flat_to_local(i);
            blocks.push(chunk.get(lx, ly, lz).unwrap_or_default());
        }
        blocks
    }

    // -----------------------------------------------------------------------
    // Round-trip correctness
    // -----------------------------------------------------------------------

    /// An all-air chunk should survive a round-trip unchanged.
    #[test]
    fn round_trip_all_air() {
        let pos = ChunkPos::new(0, 0);
        let original = Chunk::new(pos);
        let restored = round_trip(&original);

        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// A chunk with a single non-air block should round-trip correctly.
    #[test]
    fn round_trip_single_block() {
        let pos = ChunkPos::new(3, -5);
        let mut original = Chunk::new(pos);
        original.set(7, 64, 3, Block::new(BlockId(1)));

        let restored = round_trip(&original);

        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// Fill the whole chunk with a non-air block and verify correctness.
    #[test]
    fn round_trip_uniform_fill() {
        let pos = ChunkPos::new(-1, 2);
        let mut original = Chunk::new(pos);
        let stone = Block::new(BlockId(1));
        for lx in 0..CHUNK_SIZE_X {
            for ly in 0..CHUNK_SIZE_Y {
                for lz in 0..CHUNK_SIZE_Z {
                    original.set(lx, ly, lz, stone);
                }
            }
        }

        let restored = round_trip(&original);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// Realistic terrain: stone below y=64, dirt 64–66, grass at 67, air above.
    #[test]
    fn round_trip_terrain_slice() {
        let pos = ChunkPos::new(0, 0);
        let mut original = Chunk::new(pos);

        for lx in 0..CHUNK_SIZE_X {
            for lz in 0..CHUNK_SIZE_Z {
                for ly in 0..64usize {
                    original.set(lx, ly, lz, Block::new(BlockId(1))); // stone
                }
                for ly in 64..67usize {
                    original.set(lx, ly, lz, Block::new(BlockId(3))); // dirt
                }
                original.set(lx, 67, lz, Block::new(BlockId(2))); // grass
            }
        }

        let restored = round_trip(&original);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// Every block has a unique ID — worst case for RLE (no compression).
    #[test]
    fn round_trip_worst_case_rle() {
        let pos = ChunkPos::new(0, 0);
        let mut original = Chunk::new(pos);
        for i in 0..CHUNK_SIZE {
            let (lx, ly, lz) = flat_to_local(i);
            let id = ((i % (u16::MAX as usize)) + 1) as u16;
            original.set(lx, ly, lz, Block::new(BlockId(id)));
        }

        let restored = round_trip(&original);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    // -----------------------------------------------------------------------
    // Versioned serialization
    // -----------------------------------------------------------------------

    /// serialize_chunk_versioned with V1 produces the same bytes as
    /// serialize_chunk (since V1 is the current latest).
    #[test]
    fn versioned_v1_matches_latest() {
        let chunk = Chunk::new(ChunkPos::new(5, -3));
        let mut buf_latest = Vec::new();
        let mut buf_v1 = Vec::new();
        serialize_chunk(&chunk, &mut buf_latest).unwrap();
        serialize_chunk_versioned(&chunk, &mut buf_v1, ChunkVersion::V1).unwrap();
        assert_eq!(buf_latest, buf_v1);
    }

    /// A chunk written with an explicit version is readable by deserialize_chunk.
    #[test]
    fn versioned_round_trip_v1() {
        let pos = ChunkPos::new(-7, 12);
        let mut original = Chunk::new(pos);
        original.set(0, 0, 0, Block::new(BlockId(42)));

        let restored = round_trip_versioned(&original, ChunkVersion::V1);
        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// The version byte written by serialize_chunk_versioned matches the
    /// ChunkVersion variant's integer value.
    #[test]
    fn versioned_header_contains_correct_version() {
        let chunk = Chunk::new(ChunkPos::new(0, 0));
        let mut buf = Vec::new();
        serialize_chunk_versioned(&chunk, &mut buf, ChunkVersion::V1).unwrap();
        // Version is at bytes 4-5 (little-endian).
        let version = u16::from_le_bytes([buf[4], buf[5]]);
        assert_eq!(version, ChunkVersion::V1.as_u16());
    }

    // -----------------------------------------------------------------------
    // Header validation
    // -----------------------------------------------------------------------

    /// Chunk position is preserved across a round-trip for negative coordinates.
    #[test]
    fn position_preserved_negative_coords() {
        let pos = ChunkPos::new(-100, -200);
        let chunk = Chunk::new(pos);
        let restored = round_trip(&chunk);
        assert_eq!(restored.position(), pos);
    }

    /// Corrupt the magic bytes and expect [`ChunkSerializeError::BadMagic`].
    #[test]
    fn rejects_bad_magic() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0)), &mut buf).unwrap();
        buf[0] = 0xFF; // corrupt first magic byte

        let err = deserialize_chunk(buf.as_slice()).unwrap_err();
        assert!(
            matches!(err, ChunkSerializeError::BadMagic(_)),
            "expected BadMagic, got: {err}"
        );
    }

    /// Corrupt the version field and expect [`ChunkSerializeError::UnsupportedVersion`].
    #[test]
    fn rejects_unsupported_version() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0)), &mut buf).unwrap();
        // Version is at bytes 4-5 (little-endian). Write version 99.
        buf[4] = 99;
        buf[5] = 0;

        let err = deserialize_chunk(buf.as_slice()).unwrap_err();
        assert!(
            matches!(err, ChunkSerializeError::UnsupportedVersion(99)),
            "expected UnsupportedVersion(99), got: {err}"
        );
    }

    /// A truncated stream (header only, no body) should return an I/O error.
    #[test]
    fn rejects_truncated_stream() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0)), &mut buf).unwrap();
        buf.truncate(10); // keep header, strip entire body

        let err = deserialize_chunk(buf.as_slice()).unwrap_err();
        assert!(
            matches!(err, ChunkSerializeError::Io(_)),
            "expected Io error, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Encoding size
    // -----------------------------------------------------------------------

    /// An all-air chunk should encode to exactly 22 bytes:
    ///   14-byte header + 2 RLE runs × 4 bytes each.
    ///
    /// CHUNK_SIZE = 65536, MAX_RUN = 65535 → 2 runs needed.
    #[test]
    fn compact_encoding_size_air() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0)), &mut buf).unwrap();
        assert_eq!(
            buf.len(),
            22,
            "expected 22-byte encoding for all-air chunk, got {} bytes",
            buf.len()
        );
    }

    /// A fully uniform non-air chunk also encodes to 22 bytes.
    #[test]
    fn compact_encoding_uniform_non_air() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0));
        for lx in 0..CHUNK_SIZE_X {
            for ly in 0..CHUNK_SIZE_Y {
                for lz in 0..CHUNK_SIZE_Z {
                    chunk.set(lx, ly, lz, Block::new(BlockId(1)));
                }
            }
        }

        let mut buf = Vec::new();
        serialize_chunk(&chunk, &mut buf).unwrap();
        assert_eq!(
            buf.len(),
            22,
            "expected 22-byte encoding for uniform chunk, got {} bytes",
            buf.len()
        );
    }

    // -----------------------------------------------------------------------
    // flat_to_local
    // -----------------------------------------------------------------------

    /// flat_to_local must be the exact inverse of the storage index formula.
    #[test]
    fn flat_to_local_inverse_of_index() {
        for i in 0..CHUNK_SIZE {
            let (lx, ly, lz) = flat_to_local(i);
            let reconstructed = lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z;
            assert_eq!(
                reconstructed, i,
                "flat_to_local({i}) → ({lx},{ly},{lz}) does not round-trip"
            );
        }
    }
}
