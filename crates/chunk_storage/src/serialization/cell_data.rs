//! Shared on-disk codec for cell-data live state and history.
//!
//! Used by [`v2`](super::v2) and [`v2_versioned`](super::v2_versioned).
//!
//! The encoded form leans on bincode for length-prefixing and string
//! handling, so the on-wire and on-disk representations stay in sync —
//! the live cell-data state is just a `Vec` of bincode records, and the
//! history is just a `Vec<(version, SerializableCellDataChange)>`.
//!
//! # Live state record
//!
//! Each cell that has typed data writes one or more
//! [`LiveCellRecord`]s.  Multiple records share a `local` when a single
//! cell carries more than one `BlockData` type — there is no per-cell
//! grouping in the file format, which keeps encoding stateless.
//!
//! # History format
//!
//! Each entry is a `(version: u64, SerializableCellDataChange)` pair.
//! Decoding requires a [`BlockDataTypeRegistry`] so unknown types fail
//! loudly rather than silently dropping state.

use std::io::{Read, Write};

use bincode::Options;
use dd40_core::block::BlockDataTypeRegistry;
use dd40_core::chunk::wire::SerializableCellDataChange;
use dd40_core::prelude::*;
use serde::{Deserialize, Serialize};

use super::ChunkSerializeError;

/// One entry of the live cell-data section.  Multiple records may
/// share the same `local`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveCellRecord {
    local: BlockLocal,
    type_key: String,
    bytes: Vec<u8>,
}

/// Encodes the chunk's full live cell-data state.
pub(super) fn serialize_live<W: Write>(
    chunk: &Chunk,
    writer: &mut W,
) -> Result<(), ChunkSerializeError> {
    let opts = bincode::DefaultOptions::new();
    let mut records: Vec<LiveCellRecord> = Vec::new();
    for (local, value) in chunk.iter_all_cell_data() {
        let bytes = opts
            .serialize(value as &dyn BlockData)
            .map_err(|e| ChunkSerializeError::CellData(e.into()))?;
        records.push(LiveCellRecord {
            local,
            type_key: value.type_key().to_owned(),
            bytes,
        });
    }
    opts.serialize_into(writer, &records)
        .map_err(|e| ChunkSerializeError::CellData(e.into()))
}

/// Reads the live cell-data section and fills `chunk` via
/// [`Chunk::insert_cell_data_for_load`].
pub(super) fn deserialize_live<R: Read>(
    chunk: &mut Chunk,
    reader: &mut R,
    registry: &BlockDataTypeRegistry,
) -> Result<(), ChunkSerializeError> {
    let opts = bincode::DefaultOptions::new();
    let records: Vec<LiveCellRecord> = opts
        .deserialize_from(reader)
        .map_err(|e| ChunkSerializeError::CellData(e.into()))?;
    for record in records {
        let mut de = bincode::de::Deserializer::from_slice(&record.bytes, opts);
        let mut erased = <dyn erased_serde::Deserializer>::erase(&mut de);
        let value = registry
            .decode(&record.type_key, &mut erased)
            .map_err(|e| ChunkSerializeError::CellData(e.into()))?;
        chunk.insert_cell_data_for_load(record.local, value);
    }
    Ok(())
}

/// Encodes the chunk's confirmed cell-data history.
pub(super) fn serialize_history<W: Write>(
    chunk: &Chunk,
    writer: &mut W,
) -> Result<(), ChunkSerializeError> {
    let opts = bincode::DefaultOptions::new();
    let mut records: Vec<(u64, SerializableCellDataChange)> =
        Vec::with_capacity(chunk.confirmed_cell_data_history().len());
    for (version, change) in chunk.confirmed_cell_data_history().iter() {
        let s =
            SerializableCellDataChange::try_from(change).map_err(ChunkSerializeError::CellData)?;
        records.push((*version, s));
    }
    opts.serialize_into(writer, &records)
        .map_err(|e| ChunkSerializeError::CellData(e.into()))
}

/// Reads the confirmed cell-data history and pushes each entry into
/// `chunk` via [`Chunk::push_confirmed_cell_data_for_load`].
pub(super) fn deserialize_history<R: Read>(
    chunk: &mut Chunk,
    reader: &mut R,
    registry: &BlockDataTypeRegistry,
) -> Result<(), ChunkSerializeError> {
    let opts = bincode::DefaultOptions::new();
    let records: Vec<(u64, SerializableCellDataChange)> = opts
        .deserialize_from(reader)
        .map_err(|e| ChunkSerializeError::CellData(e.into()))?;
    for (version, wire) in records {
        let change = wire
            .decode(registry)
            .map_err(ChunkSerializeError::CellData)?;
        chunk.push_confirmed_cell_data_for_load(version, change);
    }
    Ok(())
}
