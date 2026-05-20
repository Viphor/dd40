//! Wire/disk representations of [`ChunkChange`] and [`CellDataChange`].
//!
//! The in-memory chunk types carry trait objects (`Box<dyn BlockData>`)
//! and runtime type identifiers ([`TypeId`]) that don't serialise.  When
//! a change crosses an out-of-process boundary (network, disk) it is
//! converted into one of the `Serializable*` types here — strings
//! replace [`TypeId`]s, [`Vec<u8>`] blobs replace boxed trait objects —
//! and converted back on the far side using a [`BlockDataTypeRegistry`].
//!
//! Conversion conventions:
//!
//! - **`SerializableChunkChange`** ↔ [`ChunkChange`] is trivially
//!   lossless, so the conversion is exposed through plain
//!   [`From`]/[`Into`] in both directions.
//! - **`SerializableCellDataChange`** ↔ [`CellDataChange`] depends on a
//!   [`BlockDataTypeRegistry`] to look up encoders/decoders, and the
//!   bincode round-trip can fail.  Encoding goes through
//!   [`TryFrom<&CellDataChange>`]; decoding requires the registry and
//!   is exposed as [`SerializableCellDataChange::decode`].
//!
//! [`TypeId`]: std::any::TypeId
//! [`BlockDataTypeRegistry`]: crate::block::BlockDataTypeRegistry

use serde::{Deserialize, Serialize};

use crate::block::{BlockData, BlockDataDecodeError, BlockDataTypeRegistry, BlockId};
use crate::chunk::change::{BlockLocal, CellDataChange, ChunkChange};

// ---------------------------------------------------------------------
// SerializableChunkChange
// ---------------------------------------------------------------------

/// Wire/disk mirror of [`ChunkChange`].
///
/// Today the in-memory and wire shapes are identical, but the type is
/// kept separate so the protocol can evolve (versioning, batching,
/// compression markers) without breaking the in-memory enum that every
/// gameplay system pattern-matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializableChunkChange {
    /// See [`ChunkChange::Place`].
    Place {
        /// Chunk-local cell being written.
        local: BlockLocal,
        /// Block being placed.
        block_id: BlockId,
    },
    /// See [`ChunkChange::Remove`].
    Remove {
        /// Chunk-local cell being cleared.
        local: BlockLocal,
    },
    /// See [`ChunkChange::Replace`].
    Replace {
        /// Chunk-local cell being overwritten.
        local: BlockLocal,
        /// Block now occupying the cell.
        new_block: BlockId,
    },
}

impl From<ChunkChange> for SerializableChunkChange {
    fn from(c: ChunkChange) -> Self {
        match c {
            ChunkChange::Place { local, block_id } => Self::Place { local, block_id },
            ChunkChange::Remove { local } => Self::Remove { local },
            ChunkChange::Replace { local, new_block } => Self::Replace { local, new_block },
        }
    }
}

impl From<SerializableChunkChange> for ChunkChange {
    fn from(n: SerializableChunkChange) -> Self {
        match n {
            SerializableChunkChange::Place { local, block_id } => Self::Place { local, block_id },
            SerializableChunkChange::Remove { local } => Self::Remove { local },
            SerializableChunkChange::Replace { local, new_block } => {
                Self::Replace { local, new_block }
            }
        }
    }
}

// ---------------------------------------------------------------------
// SerializableCellDataChange
// ---------------------------------------------------------------------

/// Wire/disk mirror of [`CellDataChange`].
///
/// The boxed trait object in [`CellDataChange::Set`] is replaced by a
/// `(type_key, bytes)` pair: `type_key` selects the decoder registered
/// in [`BlockDataTypeRegistry`], and `bytes` is the bincode-encoded
/// payload.  [`Clear`] only needs the type key.
///
/// [`Clear`]: SerializableCellDataChange::Clear
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerializableCellDataChange {
    /// Encoded form of [`CellDataChange::Set`].
    Set {
        /// Chunk-local cell.
        local: BlockLocal,
        /// `type_name` of the encoded `BlockData`.
        type_key: String,
        /// Bincode-encoded payload.  Decoded through
        /// [`BlockDataTypeRegistry::decode`].
        bytes: Vec<u8>,
    },
    /// Encoded form of [`CellDataChange::Clear`].
    Clear {
        /// Chunk-local cell.
        local: BlockLocal,
        /// `type_name` of the cleared `BlockData`.
        type_key: String,
    },
}

/// Failure modes when converting a [`CellDataChange`] to or from
/// [`SerializableCellDataChange`].
#[derive(Debug)]
pub enum CellDataWireError {
    /// Bincode failed to encode/decode the payload.
    Codec(bincode::Error),

    /// The decoder for `type_key` was not registered in
    /// [`BlockDataTypeRegistry`].
    Decode(BlockDataDecodeError),
}

impl std::fmt::Display for CellDataWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec(e) => write!(f, "bincode error: {e}"),
            Self::Decode(e) => write!(f, "block-data decode error: {e}"),
        }
    }
}

impl std::error::Error for CellDataWireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(e) => Some(e),
            Self::Decode(e) => Some(e),
        }
    }
}

impl From<bincode::Error> for CellDataWireError {
    fn from(e: bincode::Error) -> Self {
        Self::Codec(e)
    }
}

impl From<BlockDataDecodeError> for CellDataWireError {
    fn from(e: BlockDataDecodeError) -> Self {
        Self::Decode(e)
    }
}

impl TryFrom<&CellDataChange> for SerializableCellDataChange {
    type Error = CellDataWireError;

    /// Encodes a runtime [`CellDataChange`] for transmission.
    ///
    /// The boxed trait object is serialised with [`bincode`] through
    /// the [`erased_serde`] blanket impl on `dyn BlockData`.  No
    /// registry lookup is needed on the encode side — the payload
    /// carries its own [`BlockData::type_key`].
    fn try_from(change: &CellDataChange) -> Result<Self, Self::Error> {
        match change {
            CellDataChange::Set { local, value } => {
                use bincode::Options;
                let bytes =
                    bincode::DefaultOptions::new().serialize(value.as_ref() as &dyn BlockData)?;
                Ok(Self::Set {
                    local: *local,
                    type_key: value.type_key().to_owned(),
                    bytes,
                })
            }
            CellDataChange::Clear {
                local, type_key, ..
            } => Ok(Self::Clear {
                local: *local,
                type_key: (*type_key).to_owned(),
            }),
        }
    }
}

impl SerializableCellDataChange {
    /// Decodes back into a runtime [`CellDataChange`] using `registry`
    /// to look up the correct decoder for the encoded payload.
    ///
    /// This is exposed as an inherent method rather than as
    /// [`TryInto<CellDataChange>`] because the conversion requires the
    /// registry, which doesn't fit the standard library's trait
    /// signature.
    pub fn decode(
        self,
        registry: &BlockDataTypeRegistry,
    ) -> Result<CellDataChange, CellDataWireError> {
        match self {
            Self::Set {
                local,
                type_key,
                bytes,
            } => {
                let mut de =
                    bincode::de::Deserializer::from_slice(&bytes, bincode::DefaultOptions::new());
                let mut erased = <dyn erased_serde::Deserializer>::erase(&mut de);
                let value = registry.decode(&type_key, &mut erased)?;
                Ok(CellDataChange::Set { local, value })
            }
            Self::Clear { local, type_key } => {
                let info = registry
                    .get_by_key(&type_key)
                    .ok_or(BlockDataDecodeError::UnknownType(type_key.clone()))?;
                Ok(CellDataChange::clear_raw(
                    local,
                    info.type_id,
                    info.type_key,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockData, BlockDataTypeRegistry};
    use crate::chunk::change::{BlockLocal, CellDataChange, ChunkChange};
    use serde::{Deserialize, Serialize};

    fn lp(x: u8, y: u16, z: u8) -> BlockLocal {
        BlockLocal::new(x, y, z)
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct ChestState {
        slots: u32,
        label: String,
    }

    impl BlockData for ChestState {
        fn type_key(&self) -> &'static str {
            std::any::type_name::<Self>()
        }
        fn clone_box(&self) -> Box<dyn BlockData> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn serializable_chunk_change_roundtrip_place() {
        let c = ChunkChange::new_place(lp(1, 2, 3), BlockId(42));
        let n: SerializableChunkChange = c.into();
        let bytes = bincode::serialize(&n).unwrap();
        let back: SerializableChunkChange = bincode::deserialize(&bytes).unwrap();
        let r: ChunkChange = back.into();
        assert_eq!(c, r);
    }

    #[test]
    fn serializable_chunk_change_roundtrip_remove_and_replace() {
        let rm = ChunkChange::new_remove(lp(0, 1, 0));
        let rp = ChunkChange::new_replace(lp(2, 3, 4), BlockId(7));
        assert_eq!(rm, ChunkChange::from(SerializableChunkChange::from(rm)));
        assert_eq!(rp, ChunkChange::from(SerializableChunkChange::from(rp)));
    }

    fn registry_with_chest() -> BlockDataTypeRegistry {
        let mut r = BlockDataTypeRegistry::new();
        assert!(r.register::<ChestState>());
        r
    }

    #[test]
    fn serializable_cell_data_set_roundtrip() {
        let registry = registry_with_chest();
        let original = CellDataChange::new_set(
            lp(5, 6, 7),
            ChestState {
                slots: 27,
                label: "loot".into(),
            },
        );
        let wire = SerializableCellDataChange::try_from(&original).unwrap();
        let bytes = bincode::serialize(&wire).unwrap();
        let back: SerializableCellDataChange = bincode::deserialize(&bytes).unwrap();
        let runtime = back.decode(&registry).unwrap();

        match runtime {
            CellDataChange::Set { local, value } => {
                assert_eq!(local, lp(5, 6, 7));
                let chest = value.as_any().downcast_ref::<ChestState>().unwrap();
                assert_eq!(chest.slots, 27);
                assert_eq!(chest.label, "loot");
            }
            _ => panic!("expected Set"),
        }
    }

    #[test]
    fn serializable_cell_data_clear_roundtrip() {
        let registry = registry_with_chest();
        let original = CellDataChange::new_clear::<ChestState>(lp(8, 9, 10));
        let wire = SerializableCellDataChange::try_from(&original).unwrap();
        let runtime = wire.decode(&registry).unwrap();
        match runtime {
            CellDataChange::Clear {
                local,
                type_id,
                type_key,
            } => {
                assert_eq!(local, lp(8, 9, 10));
                assert_eq!(type_id, std::any::TypeId::of::<ChestState>());
                assert_eq!(type_key, std::any::type_name::<ChestState>());
            }
            _ => panic!("expected Clear"),
        }
    }

    #[test]
    fn serializable_cell_data_unknown_type_key_errors() {
        let empty_registry = BlockDataTypeRegistry::new();
        let wire = SerializableCellDataChange::Set {
            local: lp(0, 0, 0),
            type_key: "ghost::Type".into(),
            bytes: vec![1, 2, 3],
        };
        let err = wire.decode(&empty_registry).unwrap_err();
        assert!(
            matches!(err, CellDataWireError::Decode(BlockDataDecodeError::UnknownType(ref k)) if k == "ghost::Type"),
            "got {err:?}",
        );
    }

    #[test]
    fn serializable_cell_data_clear_unknown_type_key_errors() {
        let empty_registry = BlockDataTypeRegistry::new();
        let wire = SerializableCellDataChange::Clear {
            local: lp(0, 0, 0),
            type_key: "ghost::Type".into(),
        };
        let err = wire.decode(&empty_registry).unwrap_err();
        assert!(matches!(
            err,
            CellDataWireError::Decode(BlockDataDecodeError::UnknownType(_))
        ));
    }
}
