//! Config file discovery, parsing, and deep table merge.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

/// Candidates to try, in order from lowest to highest priority.
///
/// Returns `(writable_path, merged_table)` where `writable_path` is the
/// highest-priority path that either already exists or can be created (used
/// by [`crate::ConfigDisk`]).
pub(crate) fn load_all() -> (Option<PathBuf>, toml::Table) {
    let paths = discover_paths();
    let mut merged = toml::Table::new();
    let mut writable: Option<PathBuf> = None;

    for path in &paths {
        match load_file(path) {
            Some(table) => {
                deep_merge(&mut merged, table);
                writable = Some(path.clone());
            }
            None => {
                // File is a candidate write target even if it doesn't exist yet,
                // as long as we can determine a writable path.
                if writable.is_none() && can_write_to(path) {
                    writable = Some(path.clone());
                }
            }
        }
    }

    (writable, merged)
}

/// Resolves the ordered list of config file paths (layer 2 → 4).
fn discover_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Layer 2: platform config dir.
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("dd40").join("config.toml"));
    }

    // Layer 3: binary-adjacent.
    paths.push(PathBuf::from("config.toml"));

    // Layer 4: explicit env var path.
    if let Ok(explicit) = std::env::var("DD40_CONFIG") {
        if !explicit.is_empty() {
            paths.push(PathBuf::from(explicit));
        }
    }

    paths
}

/// Parse a single config file. Returns `None` and logs errors if the file
/// is absent or malformed.
fn load_file(path: &Path) -> Option<toml::Table> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => {
            info!(path = %path.display(), "loaded config file");
            s
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            error!(path = %path.display(), error = %e, "failed to read config file");
            return None;
        }
    };

    match toml::from_str::<toml::Table>(&content) {
        Ok(table) => Some(table),
        Err(e) => {
            error!(path = %path.display(), error = %e, "malformed TOML in config file; ignoring");
            None
        }
    }
}

/// Deep-merge `incoming` into `base`. For each key:
/// - If both values are tables, recurse.
/// - Otherwise, the incoming value wins (replaces).
/// - Keys only in `base` are kept unchanged.
pub(crate) fn deep_merge(base: &mut toml::Table, incoming: toml::Table) {
    for (key, incoming_val) in incoming {
        match base.get_mut(&key) {
            Some(toml::Value::Table(base_table))
                if matches!(incoming_val, toml::Value::Table(_)) =>
            {
                let toml::Value::Table(incoming_table) = incoming_val else {
                    unreachable!()
                };
                deep_merge(base_table, incoming_table);
            }
            _ => {
                base.insert(key, incoming_val);
            }
        }
    }
}

/// Returns `true` when we can expect to create/write the file at `path`.
/// Checks whether the parent directory is writable (or createable).
fn can_write_to(path: &Path) -> bool {
    let parent = path.parent().unwrap_or(Path::new("."));
    if parent.exists() {
        // Try to probe writability without actually creating the file.
        std::fs::metadata(parent)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
    } else {
        // Parent doesn't exist yet; assume we can create it.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_merge_disjoint_keys_kept() {
        let mut base: toml::Table = toml::from_str("[a]\nx = 1").unwrap();
        let incoming: toml::Table = toml::from_str("[a]\ny = 2").unwrap();
        deep_merge(&mut base, incoming);
        let a = base["a"].as_table().unwrap();
        assert_eq!(a["x"].as_integer(), Some(1));
        assert_eq!(a["y"].as_integer(), Some(2));
    }

    #[test]
    fn deep_merge_later_wins_on_conflict() {
        let mut base: toml::Table = toml::from_str("[a]\nx = 1").unwrap();
        let incoming: toml::Table = toml::from_str("[a]\nx = 99").unwrap();
        deep_merge(&mut base, incoming);
        assert_eq!(base["a"]["x"].as_integer(), Some(99));
    }

    #[test]
    fn deep_merge_base_key_survives_when_not_in_incoming() {
        let mut base: toml::Table = toml::from_str("[a]\nonly_base = true\n[b]\nz = 3").unwrap();
        let incoming: toml::Table = toml::from_str("[a]\nother = false").unwrap();
        deep_merge(&mut base, incoming);
        assert_eq!(base["a"]["only_base"].as_bool(), Some(true));
        assert_eq!(base["b"]["z"].as_integer(), Some(3));
    }

    #[test]
    fn deep_merge_non_table_wins_over_table() {
        let mut base: toml::Table = toml::from_str("[a]\nx = 1").unwrap();
        let incoming: toml::Table = toml::from_str("a = 42").unwrap();
        deep_merge(&mut base, incoming);
        assert_eq!(base["a"].as_integer(), Some(42));
    }

    #[test]
    fn load_file_returns_none_for_missing_path() {
        let result = load_file(Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_none());
    }

    #[test]
    fn load_file_returns_none_for_malformed_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, b"[[not valid toml{{{{").unwrap();
        assert!(load_file(&path).is_none());
    }

    #[test]
    fn load_file_parses_valid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, b"[network]\nrender_distance = 16\n").unwrap();
        let table = load_file(&path).unwrap();
        assert_eq!(table["network"]["render_distance"].as_integer(), Some(16));
    }
}
