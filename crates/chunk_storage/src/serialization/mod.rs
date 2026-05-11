//! Binary serialization and deserialization for [`Chunk`].
//!
//! # File format
//!
//! All multi-byte integers are stored in **little-endian** byte order.
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │  Header (18 bytes)                                 │
//! │    magic:   [u8; 4]  = 0x44 0x44 0x34 0x30         │
//! │    version: u16                                    │
//! │    chunk_x: i32                                    │
//! │    chunk_y: i32                                    │
//! │    chunk_z: i32                                    │
//! ├────────────────────────────────────────────────────┤
//! │  Body (variable, format depends on version)        │
//! │    v1            — RLE blocks + version (no history)│
//! │    v1_versioned  — RLE blocks + version + history  │
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
//! 1. Create `vN.rs` (or `vN_<flavour>.rs`) implementing `serialize_body`
//!    and `deserialize_body`.
//! 2. Add a variant to [`ChunkVersion`] with a fresh `u16` discriminant.
//! 3. Add an arm to the `match` blocks in [`serialize_chunk_versioned`] and
//!    [`deserialize_chunk`].
//! 4. Update [`LATEST_VERSION`] if the new variant should be the default.

use std::io::{self, Read, Write};

use dd40_core::prelude::*;

pub mod v1;
pub mod v1_versioned;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Four-byte magic number at the start of every chunk file: ASCII "DD40".
pub const MAGIC: [u8; 4] = [0x44, 0x44, 0x34, 0x30];

/// The version that [`serialize_chunk`] writes by default.
///
/// Bump this when adding a new version module and update the match arms
/// accordingly. The writer can still be asked to write any other
/// [`ChunkVersion`] via [`serialize_chunk_versioned`].
const LATEST_VERSION: ChunkVersion = ChunkVersion::V1;

// ---------------------------------------------------------------------------
// ChunkVersion
// ---------------------------------------------------------------------------

/// Identifies the body codec used in a chunk file.
///
/// Each variant maps 1-to-1 to a version submodule (`v1`, `v1_versioned`, …)
/// and to the integer stored in the file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkVersion {
    /// Version 1 — RLE block array followed by `version: u64`. No history.
    /// See [`v1`].
    V1 = 1,
    /// Version 1 with persisted history — RLE block array, `version: u64`,
    /// and the chunk's `confirmed_history`. See [`v1_versioned`].
    ///
    /// Choose this format when the storage backend needs to serve delta
    /// updates after a restart. Otherwise [`ChunkVersion::V1`] is smaller.
    V1Versioned = 2,
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
            2 => Ok(Self::V1Versioned),
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
/// let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
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
/// let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
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
    writer.write_all(&pos.y.to_le_bytes())?;
    writer.write_all(&pos.z.to_le_bytes())?;

    // ---- Version-specific body ----
    match version {
        ChunkVersion::V1 => v1::serialize_body(chunk, &mut writer)?,
        ChunkVersion::V1Versioned => v1_versioned::serialize_body(chunk, &mut writer)?,
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
    let chunk_y = read_i32(&mut reader)?;
    let chunk_z = read_i32(&mut reader)?;
    let pos = ChunkPos::new(chunk_x, chunk_y, chunk_z);

    // ---- Version-specific body ----
    match version {
        ChunkVersion::V1 => v1::deserialize_body(pos, &mut reader),
        ChunkVersion::V1Versioned => v1_versioned::deserialize_body(pos, &mut reader),
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

/// Maximum number of identical blocks that fit in a single RLE run.
/// Capped at `u16::MAX` because the run-length field is a `u16`.
pub(super) const MAX_RUN: usize = u16::MAX as usize;

/// Serializes the chunk's block array using run-length encoding.
///
/// Each run is written as `(run_len: u16, block_id: u16)`, both little-endian,
/// repeated until exactly [`CHUNK_SIZE`] blocks are encoded.
///
/// Shared between [`v1`] and [`v1_versioned`].
pub(super) fn serialize_rle_blocks<W: Write>(chunk: &Chunk, writer: &mut W) -> io::Result<()> {
    let mut i = 0usize;
    while i < CHUNK_SIZE {
        let (lx, ly, lz) = flat_to_local(i);
        let current_id = chunk.get(lx, ly, lz).unwrap_or_default().block_id.0;

        // Count how many following blocks share the same ID (up to MAX_RUN).
        let mut run = 1usize;
        while run < MAX_RUN && (i + run) < CHUNK_SIZE {
            let (nx, ny, nz) = flat_to_local(i + run);
            let next_id = chunk.get(nx, ny, nz).unwrap_or_default().block_id.0;
            if next_id != current_id {
                break;
            }
            run += 1;
        }

        writer.write_all(&(run as u16).to_le_bytes())?;
        writer.write_all(&current_id.to_le_bytes())?;

        i += run;
    }

    Ok(())
}

/// Deserializes an RLE-encoded block array into a fresh [`Chunk`] at `pos`.
///
/// Reads `(run_len: u16, block_id: u16)` pairs until exactly [`CHUNK_SIZE`]
/// blocks have been decoded. Shared between [`v1`] and [`v1_versioned`].
///
/// # Errors
///
/// Returns [`ChunkSerializeError::UnexpectedBlockCount`] if the runs would
/// exceed [`CHUNK_SIZE`], or [`ChunkSerializeError::Io`] on a read failure.
pub(super) fn deserialize_rle_blocks<R: Read>(
    pos: ChunkPos,
    reader: &mut R,
) -> Result<Chunk, ChunkSerializeError> {
    let mut chunk = Chunk::new(pos);
    let mut decoded = 0usize;

    while decoded < CHUNK_SIZE {
        let run = read_u16(reader)? as usize;
        let block_id = read_u16(reader)?;
        let block = Block::new(BlockId(block_id));

        let remaining = CHUNK_SIZE - decoded;
        if run > remaining {
            return Err(ChunkSerializeError::UnexpectedBlockCount {
                expected: CHUNK_SIZE,
                actual: decoded + run,
            });
        }

        for k in 0..run {
            let (lx, ly, lz) = flat_to_local(decoded + k);
            chunk.set(lx, ly, lz, block);
        }

        decoded += run;
    }

    debug_assert_eq!(decoded, CHUNK_SIZE);
    Ok(chunk)
}

#[inline(always)]
pub(super) fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

#[inline(always)]
pub(super) fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

#[inline(always)]
pub(super) fn read_u64<R: Read>(reader: &mut R) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
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
        let pos = ChunkPos::new(0, 0, 0);
        let original = Chunk::new(pos);
        let restored = round_trip(&original);

        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// A chunk with a single non-air block should round-trip correctly.
    #[test]
    fn round_trip_single_block() {
        let pos = ChunkPos::new(3, 0, -5);
        let mut original = Chunk::new(pos);
        original.set(7, 64, 3, Block::new(BlockId(1)));

        let restored = round_trip(&original);

        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// Fill the whole chunk with a non-air block and verify correctness.
    #[test]
    fn round_trip_uniform_fill() {
        let pos = ChunkPos::new(-1, 0, 2);
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
        let pos = ChunkPos::new(0, 0, 0);
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
        let pos = ChunkPos::new(0, 0, 0);
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
        let chunk = Chunk::new(ChunkPos::new(5, 0, -3));
        let mut buf_latest = Vec::new();
        let mut buf_v1 = Vec::new();
        serialize_chunk(&chunk, &mut buf_latest).unwrap();
        serialize_chunk_versioned(&chunk, &mut buf_v1, ChunkVersion::V1).unwrap();
        assert_eq!(buf_latest, buf_v1);
    }

    /// A chunk written with an explicit version is readable by deserialize_chunk.
    #[test]
    fn versioned_round_trip_v1() {
        let pos = ChunkPos::new(-7, 0, 12);
        let mut original = Chunk::new(pos);
        original.set(0, 0, 0, Block::new(BlockId(42)));
        original.set_version(7);

        let restored = round_trip_versioned(&original, ChunkVersion::V1);
        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
        assert_eq!(restored.version(), 7, "V1 must persist the chunk version");
        assert!(
            restored.confirmed_history().is_empty(),
            "V1 must drop history on save"
        );
    }

    /// V1Versioned persists the confirmed history alongside the data.
    #[test]
    fn versioned_round_trip_v1_versioned_preserves_history() {
        let pos = ChunkPos::new(2, 0, -2);
        let mut original = Chunk::new(pos);
        original.set(1, 2, 3, Block::new(BlockId(5)));
        original.set_version(42);
        original.push_confirmed_for_load(
            10,
            ChunkChange::Place {
                local: BlockLocal::new(1, 2, 3),
                block_id: BlockId(5),
            },
        );
        original.push_confirmed_for_load(
            11,
            ChunkChange::Remove {
                local: BlockLocal::new(4, 5, 6),
            },
        );
        original.push_confirmed_for_load(
            12,
            ChunkChange::Replace {
                local: BlockLocal::new(7, 8, 9),
                new_block: BlockId(99),
            },
        );

        let restored = round_trip_versioned(&original, ChunkVersion::V1Versioned);

        assert_eq!(restored.position(), pos);
        assert_eq!(all_blocks(&original), all_blocks(&restored));
        assert_eq!(restored.version(), 42);

        let restored_hist: Vec<_> = restored.confirmed_history().iter().copied().collect();
        let original_hist: Vec<_> = original.confirmed_history().iter().copied().collect();
        assert_eq!(restored_hist, original_hist);
    }

    /// V1Versioned with no history round-trips identically to V1 in payload
    /// (modulo the four trailing zero-length bytes for `history_len = 0`).
    #[test]
    fn versioned_round_trip_v1_versioned_with_empty_history() {
        let pos = ChunkPos::new(0, 0, 0);
        let mut original = Chunk::new(pos);
        original.set_version(1);

        let restored = round_trip_versioned(&original, ChunkVersion::V1Versioned);
        assert_eq!(restored.version(), 1);
        assert!(restored.confirmed_history().is_empty());
        assert_eq!(all_blocks(&original), all_blocks(&restored));
    }

    /// The version byte written by serialize_chunk_versioned matches the
    /// ChunkVersion variant's integer value.
    #[test]
    fn versioned_header_contains_correct_version() {
        let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
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
        let pos = ChunkPos::new(-100, 0, -200);
        let chunk = Chunk::new(pos);
        let restored = round_trip(&chunk);
        assert_eq!(restored.position(), pos);
    }

    /// Corrupt the magic bytes and expect [`ChunkSerializeError::BadMagic`].
    #[test]
    fn rejects_bad_magic() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0, 0)), &mut buf).unwrap();
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
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0, 0)), &mut buf).unwrap();
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
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0, 0)), &mut buf).unwrap();
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

    /// An all-air chunk should encode to exactly 34 bytes:
    ///   18-byte header + 2 RLE runs × 4 bytes + 8-byte version.
    ///
    /// Header = 4 (magic) + 2 (version tag) + 4 (x) + 4 (y) + 4 (z) = 18.
    /// CHUNK_SIZE = 65536, MAX_RUN = 65535 → 2 runs needed.
    #[test]
    fn compact_encoding_size_air() {
        let mut buf = Vec::new();
        serialize_chunk(&Chunk::new(ChunkPos::new(0, 0, 0)), &mut buf).unwrap();
        assert_eq!(
            buf.len(),
            34,
            "expected 34-byte encoding for all-air chunk, got {} bytes",
            buf.len()
        );
    }

    /// A fully uniform non-air chunk also encodes to 34 bytes.
    #[test]
    fn compact_encoding_uniform_non_air() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
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
            34,
            "expected 34-byte encoding for uniform chunk, got {} bytes",
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
