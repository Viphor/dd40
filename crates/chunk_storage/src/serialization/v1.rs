//! Version 1 chunk body codec — run-length encoded block IDs.
//!
//! # Body format
//!
//! The body immediately follows the shared 10-byte header written by the
//! parent module.  It consists of a sequence of `(run_len: u16, block_id: u16)`
//! pairs in little-endian byte order, repeated until exactly [`CHUNK_SIZE`]
//! blocks have been decoded.
//!
//! ```text
//! Repeating until CHUNK_SIZE blocks decoded:
//!   run_len:  u16  — number of consecutive identical blocks (1 – 65535)
//!   block_id: u16  — the block type for this run
//! ```
//!
//! Block order matches the chunk's flat-array layout:
//! `index = lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z`

use std::io::{self, Read, Write};

use dd40_core::prelude::*;

use crate::serialization::ChunkSerializeError;

use super::flat_to_local;

/// Maximum number of identical blocks that fit in a single RLE run.
/// Capped at `u16::MAX` because the run-length field is a `u16`.
const MAX_RUN: usize = u16::MAX as usize;

/// Serializes the body of `chunk` (everything after the shared header) into
/// `writer` using run-length encoding.
///
/// Each run is written as `(run_len: u16, block_id: u16)`, both little-endian.
///
/// # Errors
///
/// Returns an [`io::Error`] if any write to `writer` fails.
pub(super) fn serialize_body<W: Write>(chunk: &Chunk, writer: &mut W) -> io::Result<()> {
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

        // Write (run_length: u16, block_id: u16).
        writer.write_all(&(run as u16).to_le_bytes())?;
        writer.write_all(&current_id.to_le_bytes())?;

        i += run;
    }

    Ok(())
}

/// Deserializes the body of a chunk (everything after the shared header) from
/// `reader` into a pre-allocated [`Chunk`] at `pos`.
///
/// # Errors
///
/// - [`io::Error`] — any read failure, including unexpected EOF.
/// - Returns an `UnexpectedBlockCount` error (via the caller) if the number of
///   decoded blocks does not equal [`CHUNK_SIZE`]; this is propagated as an
///   `io::Error` with `InvalidData` kind so the parent can wrap it uniformly.
pub(super) fn deserialize_body<R: Read>(
    pos: ChunkPos,
    reader: &mut R,
) -> Result<Chunk, ChunkSerializeError> {
    let mut chunk = Chunk::new(pos);
    let mut decoded = 0usize;

    while decoded < CHUNK_SIZE {
        let run = read_u16(reader)? as usize;
        let block_id = read_u16(reader)?;
        let block = Block::new(BlockId(block_id));

        // Guard against a malformed stream whose runs would exceed CHUNK_SIZE.
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

    // decoded == CHUNK_SIZE is guaranteed by the loop condition, but we check
    // explicitly so that any future refactor of the loop cannot silently skip it.
    debug_assert_eq!(decoded, CHUNK_SIZE);

    Ok(chunk)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

#[inline(always)]
fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}
