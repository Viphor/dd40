//! Version 1 chunk body codec — RLE blocks + chunk version, no history.
//!
//! # Body format
//!
//! The body immediately follows the shared header written by the parent
//! module. It consists of:
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │  RLE block array                                   │
//! │    Repeating until CHUNK_SIZE blocks decoded:      │
//! │      run_len:  u16                                 │
//! │      block_id: u16                                 │
//! ├────────────────────────────────────────────────────┤
//! │  Chunk version                                     │
//! │      version: u64                                  │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! All multi-byte integers are little-endian. The `version` field carries the
//! `Chunk::version()` value at write time so the caller can resume from the
//! correct authoritative version on load.
//!
//! Block order matches the chunk's flat-array layout:
//! `index = lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z`

use std::io::{self, Read, Write};

use dd40_core::prelude::*;

use crate::serialization::ChunkSerializeError;

use super::{
    deserialize_rle_blocks, read_u64, serialize_rle_blocks,
};

/// Serializes the body of `chunk` (RLE blocks + chunk version).
///
/// # Errors
///
/// Returns an [`io::Error`] if any write to `writer` fails.
pub(super) fn serialize_body<W: Write>(chunk: &Chunk, writer: &mut W) -> io::Result<()> {
    serialize_rle_blocks(chunk, writer)?;
    writer.write_all(&chunk.version().to_le_bytes())?;
    Ok(())
}

/// Deserializes the body of a V1 chunk into a fresh [`Chunk`] at `pos`.
///
/// The decoded chunk has its `version` set from the trailing field and an
/// empty `confirmed_history`.
///
/// # Errors
///
/// - [`io::Error`] — any read failure, including unexpected EOF.
/// - Returns an `UnexpectedBlockCount` error (via the caller) if the number of
///   decoded blocks does not equal [`CHUNK_SIZE`].
pub(super) fn deserialize_body<R: Read>(
    pos: ChunkPos,
    reader: &mut R,
) -> Result<Chunk, ChunkSerializeError> {
    let mut chunk = deserialize_rle_blocks(pos, reader)?;
    let version = read_u64(reader)?;
    chunk.set_version(version);
    Ok(chunk)
}
