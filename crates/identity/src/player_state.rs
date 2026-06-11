use std::io::{Read, Write};
use std::path::Path;

use bevy::prelude::warn;
use dd40_identity_core::PlayerSaveState;

const MAGIC: [u8; 6] = *b"DD40PL";
const VERSION: u16 = 1;

/// Sanitises a `sub` string so it is safe to use as a file-system path
/// component.
///
/// Any character that is not alphanumeric, `-`, or `_` is replaced with `_`.
/// This prevents path-traversal attacks.
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

/// Loads a player's save state from `dir/<sub>.bin`.
///
/// Returns `None` if the file does not exist or cannot be parsed. Errors
/// are logged at `warn!`.
pub fn load(dir: &Path, sub: &str) -> Option<PlayerSaveState> {
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

    let version = u16::from_le_bytes([data[6], data[7]]);
    match version {
        1 => match bincode::deserialize::<PlayerSaveState>(&data[8..]) {
            Ok(state) => Some(state),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to deserialise player save");
                None
            }
        },
        v => {
            warn!(path = %path.display(), version = v, "unknown player save version");
            None
        }
    }
}

/// Saves a player's state to `dir/<sub>.bin`.
///
/// Creates the directory if it does not exist.
pub fn save(dir: &Path, sub: &str, state: &PlayerSaveState) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.bin", sanitise_sub(sub)));

    let body = bincode::serialize(state)
        .map_err(|e| std::io::Error::other(format!("serialisation failed: {e}")))?;

    let mut buf = Vec::with_capacity(8 + body.len());
    buf.write_all(&MAGIC)?;
    buf.write_all(&VERSION.to_le_bytes())?;
    buf.write_all(&body)?;

    std::fs::write(&path, &buf)
}

#[cfg(test)]
mod tests {
    use dd40_identity_core::{PlayerSaveState, Vec3Serde};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn round_trip() {
        let dir = TempDir::new().unwrap();
        let state = PlayerSaveState {
            last_position: Vec3Serde {
                x: 1.0,
                y: 64.0,
                z: -5.5,
            },
            inventory: vec![],
        };

        save(dir.path(), "test-sub", &state).unwrap();
        let loaded = load(dir.path(), "test-sub").unwrap();

        assert_eq!(loaded.last_position, state.last_position);
        assert_eq!(loaded.inventory, state.inventory);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = TempDir::new().unwrap();
        assert!(load(dir.path(), "nobody").is_none());
    }

    #[test]
    fn path_traversal_sanitised() {
        let dir = TempDir::new().unwrap();
        let state = PlayerSaveState::default();
        let malicious_sub = "../../../etc/passwd";
        save(dir.path(), malicious_sub, &state).unwrap();
        // The sanitised filename must exist inside the dir (no escape).
        let sanitised = malicious_sub
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        let file = dir.path().join(format!("{sanitised}.bin"));
        assert!(file.exists(), "expected file at {}", file.display());
    }
}
