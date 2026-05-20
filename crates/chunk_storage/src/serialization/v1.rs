//! Version 1 chunk body codec — RLE blocks + chunk version + live cell data.
//!
//! V1 is the baseline persistent format: it preserves the chunk's block grid,
//! the current authoritative `version`, and the live cell-data state (typed
//! `BlockData` entries against individual cells, e.g. chest inventories or
//! sign text). No history (block or cell-data) is written — use
//! [`v1_versioned`](super::v1_versioned) when delta replay across restarts
//! is required.
//!
//! # Body format
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
//! ├────────────────────────────────────────────────────┤
//! │  Live cell data (bincode-encoded record list)      │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! All multi-byte integers are little-endian. Block order matches the
//! chunk's flat-array layout:
//! `index = lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z`.

use std::io::{Read, Write};

use dd40_core::block::BlockDataTypeRegistry;
use dd40_core::prelude::*;

use super::{
    ChunkSerializeError, cell_data, deserialize_rle_blocks, read_u64, serialize_rle_blocks,
};

/// Serializes the body of `chunk` (RLE blocks + version + live cell data).
pub(super) fn serialize_body<W: Write>(
    chunk: &Chunk,
    writer: &mut W,
) -> Result<(), ChunkSerializeError> {
    serialize_rle_blocks(chunk, writer)?;
    writer.write_all(&chunk.version().to_le_bytes())?;
    cell_data::serialize_live(chunk, writer)?;
    Ok(())
}

/// Deserializes a V1 body into a fresh [`Chunk`] at `pos`.
pub(super) fn deserialize_body<R: Read>(
    pos: ChunkPos,
    reader: &mut R,
    registry: &BlockDataTypeRegistry,
) -> Result<Chunk, ChunkSerializeError> {
    let mut chunk = deserialize_rle_blocks(pos, reader)?;
    let version = read_u64(reader)?;
    chunk.set_version(version);
    cell_data::deserialize_live(&mut chunk, reader, registry)?;
    Ok(chunk)
}
