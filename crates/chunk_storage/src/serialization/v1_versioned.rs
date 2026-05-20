//! Version 1 (history-preserving) chunk body codec — V1 body plus the
//! chunk's confirmed block-change history and its confirmed cell-data
//! history.
//!
//! Pick this format when the storage backend needs to serve delta
//! updates after a restart. Otherwise [`v1`](super::v1) is smaller.
//!
//! # Body format
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │  RLE block array                                   │
//! ├────────────────────────────────────────────────────┤
//! │  Chunk version: u64                                │
//! ├────────────────────────────────────────────────────┤
//! │  Block confirmed history                           │
//! │      history_len: u32                              │
//! │      Repeated history_len times:                   │
//! │        version_at_change: u64                      │
//! │        change: 8 bytes (see `serialize_change`)    │
//! ├────────────────────────────────────────────────────┤
//! │  Live cell data       (bincode record list)        │
//! ├────────────────────────────────────────────────────┤
//! │  Cell-data history    (bincode record list)        │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! All multi-byte integers are little-endian. Each [`ChunkChange`] is
//! encoded in a fixed 8 bytes:
//!
//! ```text
//!   tag:      u8  (0 = Place, 1 = Remove, 2 = Replace)
//!   lx:       u8
//!   ly:       u16
//!   lz:       u8
//!   block_id: u16  (zero for Remove)
//!   _pad:     u8   (reserved, must be 0)
//! ```

use std::io::{self, Read, Write};

use dd40_core::block::BlockDataTypeRegistry;
use dd40_core::prelude::*;

use super::{
    ChunkSerializeError, cell_data, deserialize_rle_blocks, read_u16, read_u32, read_u64,
    serialize_rle_blocks,
};

const TAG_PLACE: u8 = 0;
const TAG_REMOVE: u8 = 1;
const TAG_REPLACE: u8 = 2;

/// Serializes the body of `chunk` (RLE blocks + version + block history
/// + live cell data + cell-data history).
pub(super) fn serialize_body<W: Write>(
    chunk: &Chunk,
    writer: &mut W,
) -> Result<(), ChunkSerializeError> {
    serialize_rle_blocks(chunk, writer)?;
    writer.write_all(&chunk.version().to_le_bytes())?;

    let history = chunk.confirmed_history();
    let len: u32 = history
        .len()
        .try_into()
        .expect("confirmed_history length exceeds u32::MAX (~4 billion entries)");
    writer.write_all(&len.to_le_bytes())?;

    for (version, change) in history.iter() {
        writer.write_all(&version.to_le_bytes())?;
        serialize_change(*change, writer)?;
    }

    cell_data::serialize_live(chunk, writer)?;
    cell_data::serialize_history(chunk, writer)?;
    Ok(())
}

/// Deserializes a V1Versioned body into a fresh [`Chunk`] at `pos`.
pub(super) fn deserialize_body<R: Read>(
    pos: ChunkPos,
    reader: &mut R,
    registry: &BlockDataTypeRegistry,
) -> Result<Chunk, ChunkSerializeError> {
    let mut chunk = deserialize_rle_blocks(pos, reader)?;
    let version = read_u64(reader)?;
    chunk.set_version(version);

    let history_len = read_u32(reader)? as usize;
    for _ in 0..history_len {
        let change_version = read_u64(reader)?;
        let change = deserialize_change(reader)?;
        chunk.push_confirmed_for_load(change_version, change);
    }

    cell_data::deserialize_live(&mut chunk, reader, registry)?;
    cell_data::deserialize_history(&mut chunk, reader, registry)?;
    Ok(chunk)
}

fn serialize_change<W: Write>(change: ChunkChange, writer: &mut W) -> io::Result<()> {
    let (tag, local, block_id) = match change {
        ChunkChange::Place { local, block_id } => (TAG_PLACE, local, block_id.0),
        ChunkChange::Remove { local } => (TAG_REMOVE, local, 0),
        ChunkChange::Replace { local, new_block } => (TAG_REPLACE, local, new_block.0),
    };
    writer.write_all(&[tag])?;
    writer.write_all(&[local.x])?;
    writer.write_all(&local.y.to_le_bytes())?;
    writer.write_all(&[local.z])?;
    writer.write_all(&block_id.to_le_bytes())?;
    writer.write_all(&[0u8])?; // reserved padding for future use
    Ok(())
}

fn deserialize_change<R: Read>(reader: &mut R) -> Result<ChunkChange, ChunkSerializeError> {
    let mut tag = [0u8; 1];
    reader.read_exact(&mut tag)?;
    let mut lx = [0u8; 1];
    reader.read_exact(&mut lx)?;
    let ly = read_u16(reader)?;
    let mut lz = [0u8; 1];
    reader.read_exact(&mut lz)?;
    let block_id = read_u16(reader)?;
    let mut _pad = [0u8; 1];
    reader.read_exact(&mut _pad)?;

    let local = BlockLocal::try_new(lx[0], ly, lz[0]).ok_or_else(|| {
        ChunkSerializeError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid BlockLocal in history: ({}, {}, {})",
                lx[0], ly, lz[0]
            ),
        ))
    })?;

    Ok(match tag[0] {
        TAG_PLACE => ChunkChange::Place {
            local,
            block_id: BlockId(block_id),
        },
        TAG_REMOVE => ChunkChange::Remove { local },
        TAG_REPLACE => ChunkChange::Replace {
            local,
            new_block: BlockId(block_id),
        },
        other => {
            return Err(ChunkSerializeError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown ChunkChange tag: {other}"),
            )));
        }
    })
}
