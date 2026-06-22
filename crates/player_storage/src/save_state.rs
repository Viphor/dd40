use std::io::{Read, Write};
use std::path::Path;

use bevy::math::Vec3;
use bevy::prelude::warn;
use serde::{Deserialize, Serialize};

/// The complete persisted state for one player, written to
/// `<PlayersDir>/<sanitised_sub>.bin`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerSaveState {
    /// Last known world-space position.
    pub last_position: Vec3Serde,
    /// Versioned blobs from each registered contributor.
    ///
    /// Each entry is `(contributor kind, [u16 LE version][payload])`.
    pub blobs: Vec<(String, Vec<u8>)>,
}

/// `Vec3` serialisation shim.
///
/// Bevy's `Vec3` does not implement `serde::Serialize`/`Deserialize` in all
/// feature configurations, so we use this plain struct for the save file
/// and convert with [`From`] impls.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Vec3Serde {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<Vec3> for Vec3Serde {
    fn from(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<Vec3Serde> for Vec3 {
    fn from(v: Vec3Serde) -> Self {
        Vec3::new(v.x, v.y, v.z)
    }
}

const MAGIC: [u8; 6] = *b"DD40PS";
const VERSION: u16 = 1;

fn sanitise_sub(sub: &str) -> String {
    sub.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Load the save state for `sub` from `dir`, or `None` if the file does not
/// exist or cannot be parsed.
pub fn load_player_state(dir: &Path, sub: &str) -> Option<PlayerSaveState> {
    let path = dir.join(format!("{}.bin", sanitise_sub(sub)));
    let mut data = Vec::new();

    match std::fs::File::open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.read_to_end(&mut data) {
                warn!(path = %path.display(), error = %e, "failed to read player save file");
                return None;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "failed to open player save file");
            return None;
        }
    }

    if data.len() < 8 {
        warn!(path = %path.display(), "player save file too short");
        return None;
    }

    if data[0..6] != MAGIC {
        warn!(path = %path.display(), "player save file has wrong magic bytes");
        return None;
    }

    let file_version = u16::from_le_bytes([data[6], data[7]]);
    match file_version {
        1 => match bincode::deserialize::<PlayerSaveState>(&data[8..]) {
            Ok(state) => Some(state),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to deserialise player save");
                None
            }
        },
        v => {
            warn!(path = %path.display(), version = v, "unknown player save file version");
            None
        }
    }
}

/// Persist `state` for `sub` into `dir`.
pub fn save_player_state(
    dir: &Path,
    sub: &str,
    state: &PlayerSaveState,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.bin", sanitise_sub(sub)));

    let body = bincode::serialize(state)
        .map_err(|e| std::io::Error::other(format!("serialisation failed: {e}")))?;

    let mut buf = Vec::with_capacity(8 + body.len());
    buf.write_all(&MAGIC)?;
    buf.write_all(&VERSION.to_le_bytes())?;
    buf.write_all(&body)?;

    std::fs::write(path, buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty() {
        let dir = tempfile::tempdir().unwrap();
        let state = PlayerSaveState {
            last_position: Vec3Serde {
                x: 1.0,
                y: 64.0,
                z: -5.5,
            },
            blobs: vec![],
        };

        save_player_state(dir.path(), "test-user", &state).unwrap();
        let loaded = load_player_state(dir.path(), "test-user").unwrap();
        assert_eq!(loaded.last_position, state.last_position);
        assert_eq!(loaded.blobs, state.blobs);
    }

    #[test]
    fn round_trip_with_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let versioned = {
            let version: u16 = 1;
            let payload = vec![1u8, 2, 3, 4];
            let mut v = Vec::with_capacity(2 + payload.len());
            v.extend_from_slice(&version.to_le_bytes());
            v.extend_from_slice(&payload);
            v
        };
        let state = PlayerSaveState {
            last_position: Vec3Serde::default(),
            blobs: vec![("inventory".to_string(), versioned)],
        };

        save_player_state(dir.path(), "test-user", &state).unwrap();
        let loaded = load_player_state(dir.path(), "test-user").unwrap();
        assert_eq!(loaded.blobs, state.blobs);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_player_state(dir.path(), "nobody").is_none());
    }
}
