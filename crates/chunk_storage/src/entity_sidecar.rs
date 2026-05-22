//! Binary serialization for the per-chunk **entity sidecar** file.
//!
//! The sidecar lives next to each chunk file on disk and stores the
//! [`PersistedEntity`] payloads produced by every registered
//! [`EntityPersister`] for that chunk:
//!
//! ```text
//! <dir>/chunk_<x>_<y>_<z>.bin       ← block data
//! <dir>/entities_<x>_<y>_<z>.bin    ← entity sidecar (this module)
//! ```
//!
//! # File format
//!
//! All multi-byte integers are stored in **little-endian** byte order.
//!
//! ```text
//! ┌────────────────────────────────────────────────────┐
//! │  Header (18 bytes)                                 │
//! │    magic:   [u8; 4]  = 0x44 0x44 0x34 0x30 ("DD40")│
//! │    version: u16                                    │
//! │    chunk_x: i32                                    │
//! │    chunk_y: i32                                    │
//! │    chunk_z: i32                                    │
//! ├────────────────────────────────────────────────────┤
//! │  Body (bincode-encoded `Vec<PersistedEntity>`)     │
//! └────────────────────────────────────────────────────┘
//! ```
//!
//! The magic + chunk-coordinate header mirrors the block-chunk file
//! format so an admin reading raw bytes can tell at a glance that
//! `chunk_3_0_-2.bin` and `entities_3_0_-2.bin` are siblings.
//!
//! The version field reserves room for format evolution: today only
//! [`EntitiesVersion::V1`] exists, but a future variant can extend
//! the schema (e.g. compression, per-entity timestamps) without
//! breaking older saves.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use dd40_core::prelude::*;

/// Magic number written at the start of every entity sidecar — ASCII "DD40".
/// Identical to the chunk file's magic on purpose: the differentiator
/// is the filename prefix, not the magic.
pub const ENTITY_SIDECAR_MAGIC: [u8; 4] = [0x44, 0x44, 0x34, 0x30];

/// Header length in bytes (4-byte magic + 2-byte version + 3×4-byte chunk coordinate).
pub const ENTITY_SIDECAR_HEADER_LEN: usize = 4 + 2 + 4 + 4 + 4;

/// On-disk version tag for the entity sidecar body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitiesVersion {
    /// Version 1 — bincode-encoded `Vec<PersistedEntity>`.
    V1 = 1,
}

impl EntitiesVersion {
    /// Returns the integer representation written into the file header.
    pub fn as_u16(self) -> u16 {
        self as u16
    }

    /// Decodes a raw header version integer.
    pub fn from_u16(raw: u16) -> Result<Self, EntitySidecarError> {
        match raw {
            1 => Ok(Self::V1),
            other => Err(EntitySidecarError::UnsupportedVersion(other)),
        }
    }
}

/// Errors produced while reading or writing an entity sidecar.
#[derive(Debug)]
pub enum EntitySidecarError {
    /// File header did not start with [`ENTITY_SIDECAR_MAGIC`].
    BadMagic([u8; 4]),
    /// Version field is not recognised by this build.
    UnsupportedVersion(u16),
    /// Coordinates in the header do not match the chunk the file
    /// claims to belong to (filename mismatch).
    CoordinateMismatch {
        expected: ChunkPos,
        found: ChunkPos,
    },
    /// Underlying I/O error.
    Io(io::Error),
    /// Bincode decode/encode failure on the body.
    Bincode(bincode::Error),
}

impl std::fmt::Display for EntitySidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic(b) => write!(f, "entity sidecar has bad magic: {b:?}"),
            Self::UnsupportedVersion(v) => write!(f, "entity sidecar version {v} is not supported"),
            Self::CoordinateMismatch { expected, found } => write!(
                f,
                "entity sidecar coordinate mismatch: file says {found}, expected {expected}"
            ),
            Self::Io(e) => write!(f, "entity sidecar I/O error: {e}"),
            Self::Bincode(e) => write!(f, "entity sidecar bincode error: {e}"),
        }
    }
}

impl std::error::Error for EntitySidecarError {}

impl From<io::Error> for EntitySidecarError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<bincode::Error> for EntitySidecarError {
    fn from(value: bincode::Error) -> Self {
        Self::Bincode(value)
    }
}

/// Computes the on-disk path for a chunk's entity sidecar inside `dir`.
///
/// Mirrors the `chunk_X_Y_Z.bin` naming used for block data so that
/// every loaded chunk has an obvious sibling sidecar.
pub fn entity_sidecar_path(dir: &Path, pos: ChunkPos) -> PathBuf {
    dir.join(format!("entities_{}_{}_{}.bin", pos.x, pos.y, pos.z))
}

/// Writes a sidecar containing `entities` for `pos` to `writer`.
///
/// The writer is consumed once: caller is responsible for buffering
/// (typically `BufWriter::new(File::create(path)?)`).
pub fn serialize_entities<W: Write>(
    mut writer: W,
    pos: ChunkPos,
    entities: &[PersistedEntity],
) -> Result<(), EntitySidecarError> {
    writer.write_all(&ENTITY_SIDECAR_MAGIC)?;
    writer.write_all(&EntitiesVersion::V1.as_u16().to_le_bytes())?;
    writer.write_all(&pos.x.to_le_bytes())?;
    writer.write_all(&pos.y.to_le_bytes())?;
    writer.write_all(&pos.z.to_le_bytes())?;
    bincode::serialize_into(&mut writer, entities)?;
    Ok(())
}

/// Reads a sidecar from `reader`, returning the parsed entities.
///
/// Verifies the magic, version, and that the coordinates in the
/// header match `expected` (so files moved into a wrong directory
/// fail loudly rather than spawning duplicates in the wrong chunk).
pub fn deserialize_entities<R: Read>(
    mut reader: R,
    expected: ChunkPos,
) -> Result<Vec<PersistedEntity>, EntitySidecarError> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != ENTITY_SIDECAR_MAGIC {
        return Err(EntitySidecarError::BadMagic(magic));
    }

    let mut version_bytes = [0u8; 2];
    reader.read_exact(&mut version_bytes)?;
    let _ = EntitiesVersion::from_u16(u16::from_le_bytes(version_bytes))?;

    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    let x = i32::from_le_bytes(buf);
    reader.read_exact(&mut buf)?;
    let y = i32::from_le_bytes(buf);
    reader.read_exact(&mut buf)?;
    let z = i32::from_le_bytes(buf);

    let found = ChunkPos::new(x, y, z);
    if found != expected {
        return Err(EntitySidecarError::CoordinateMismatch { expected, found });
    }

    let entities: Vec<PersistedEntity> = bincode::deserialize_from(&mut reader)?;
    Ok(entities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_entity(kind: &str, payload: &[u8]) -> PersistedEntity {
        PersistedEntity {
            kind: kind.to_string(),
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn roundtrip_preserves_entities_and_coordinates() {
        let pos = ChunkPos::new(3, -1, 7);
        let entities = vec![
            sample_entity("loose_item_core.loose_item", &[1, 2, 3, 4]),
            sample_entity("npc.zombie", &[]),
        ];

        let mut buf = Vec::new();
        serialize_entities(&mut buf, pos, &entities).unwrap();
        let parsed = deserialize_entities(Cursor::new(&buf), pos).unwrap();
        assert_eq!(parsed, entities);
    }

    #[test]
    fn header_starts_with_magic_and_version() {
        let mut buf = Vec::new();
        serialize_entities(&mut buf, ChunkPos::new(0, 0, 0), &[]).unwrap();
        assert_eq!(&buf[0..4], &ENTITY_SIDECAR_MAGIC);
        assert_eq!(&buf[4..6], &1u16.to_le_bytes());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0, 0, 0, 0]);
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&[0i32.to_le_bytes(); 3].concat());
        let err = deserialize_entities(Cursor::new(buf), ChunkPos::new(0, 0, 0)).unwrap_err();
        assert!(matches!(err, EntitySidecarError::BadMagic(_)));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&ENTITY_SIDECAR_MAGIC);
        buf.extend_from_slice(&999u16.to_le_bytes());
        buf.extend_from_slice(&[0i32.to_le_bytes(); 3].concat());
        let err = deserialize_entities(Cursor::new(buf), ChunkPos::new(0, 0, 0)).unwrap_err();
        assert!(matches!(err, EntitySidecarError::UnsupportedVersion(999)));
    }

    #[test]
    fn coordinate_mismatch_is_rejected() {
        let mut buf = Vec::new();
        serialize_entities(&mut buf, ChunkPos::new(1, 0, 2), &[]).unwrap();
        let err = deserialize_entities(Cursor::new(buf), ChunkPos::new(5, 0, 5)).unwrap_err();
        match err {
            EntitySidecarError::CoordinateMismatch { expected, found } => {
                assert_eq!(expected, ChunkPos::new(5, 0, 5));
                assert_eq!(found, ChunkPos::new(1, 0, 2));
            }
            other => panic!("expected CoordinateMismatch, got {other:?}"),
        }
    }

    #[test]
    fn path_uses_chunk_coordinates() {
        let path = entity_sidecar_path(Path::new("/tmp/saves"), ChunkPos::new(3, -1, 7));
        assert_eq!(path.file_name().unwrap(), "entities_3_-1_7.bin");
    }
}
