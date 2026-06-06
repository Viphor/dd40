//! [`ConfigDisk`] and [`save_config_section`] — layer-aware, round-trip-safe saves.

use std::io;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::ConfigSection;

/// The writable config target and the base table for delta-save computation.
///
/// Inserted by [`crate::ConfigPlugin`]. Absent when no writable path could be
/// determined (e.g. running in a read-only environment).
#[derive(Resource, Clone, Debug)]
pub struct ConfigDisk {
    /// The path where [`save_config_section`] writes.
    pub path: PathBuf,
    /// Merged table from all layers with lower priority than `path`.
    /// Keys whose value matches this base are not written to `path`,
    /// so lower-layer files remain authoritative for unchanged settings.
    pub(crate) base: toml::Table,
}

/// Error returned by [`save_config_section`].
#[derive(Debug)]
pub enum ConfigSaveError {
    /// The config section could not be serialized.
    Serialize(toml::ser::Error),
    /// An I/O error occurred while writing the file.
    Io(io::Error),
}

impl std::fmt::Display for ConfigSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialization error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for ConfigSaveError {}

impl From<io::Error> for ConfigSaveError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Write section `T` back to `disk.path` using a delta-aware, round-trip-safe
/// merge.
///
/// ## What is written
///
/// For each key in the serialized `value`:
/// - If the key **already exists** in the save-target file → update it (the user
///   explicitly overrode this key before).
/// - Else if the new value **differs from the base** → write it (recording a new
///   override).
/// - Else → omit it (the lower layer is authoritative; no shadowing).
///
/// Keys in the save-target file that are not in `value` (unknown/future fields)
/// are left untouched. All other sections are also preserved.
///
/// ## Atomicity
///
/// Writes to a temp file alongside `disk.path`, then renames. The original file
/// is untouched if the write fails.
pub fn save_config_section<T: ConfigSection>(
    disk: &ConfigDisk,
    value: &T,
) -> Result<(), ConfigSaveError> {
    // Serialize the new section values into a TOML Table.
    let new_section: toml::Table = {
        let val = toml::Value::try_from(value).map_err(ConfigSaveError::Serialize)?;
        match val {
            toml::Value::Table(t) => t,
            other => {
                let mut t = toml::Table::new();
                t.insert(T::SECTION.to_string(), other);
                t
            }
        }
    };

    // Read the existing save-target file (or start empty).
    let mut file_table = read_table_or_empty(&disk.path);

    // Get the existing section in the save-target (what the user previously
    // wrote there) and the base section (what lower layers provide).
    let existing_section = section_table(&file_table, T::SECTION).clone();
    let base_section = section_table(&disk.base, T::SECTION).clone();

    // Build the updated section by applying the delta.
    let mut updated_section = existing_section.clone();
    for (key, new_val) in new_section {
        let already_in_target = existing_section.contains_key(&key);
        let matches_base = base_section
            .get(&key)
            .map(|b| *b == new_val)
            .unwrap_or(false);

        if already_in_target || !matches_base {
            updated_section.insert(key, new_val);
        }
        // else: value matches base and is not in the target → omit.
    }

    // Update the section in the file table.
    file_table.insert(
        T::SECTION.to_string(),
        toml::Value::Table(updated_section),
    );

    // Serialize and write atomically.
    let content = toml::to_string_pretty(&file_table)
        .map_err(ConfigSaveError::Serialize)?;
    write_atomic(&disk.path, content.as_bytes())?;

    Ok(())
}

/// Read a `toml::Table` from `path`, returning an empty table on any error.
fn read_table_or_empty(path: &Path) -> toml::Table {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str::<toml::Table>(&s).unwrap_or_default(),
        Err(_) => toml::Table::new(),
    }
}

/// Get a reference to the named section table, or an empty one.
fn section_table<'a>(table: &'a toml::Table, key: &str) -> &'a toml::Table {
    table
        .get(key)
        .and_then(|v| v.as_table())
        .map(|t| t)
        .unwrap_or(&EMPTY_TABLE)
}

static EMPTY_TABLE: std::sync::LazyLock<toml::Table> =
    std::sync::LazyLock::new(toml::Table::new);

/// Write `content` to `path` atomically via a sibling temp file + rename.
fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
    #[serde(default)]
    struct NetCfg {
        render_distance: i32,
        extra: String,
    }

    impl ConfigSection for NetCfg {
        const SECTION: &'static str = "network";
    }

    fn make_disk(dir: &std::path::Path, base_toml: &str) -> ConfigDisk {
        ConfigDisk {
            path: dir.join("config.toml"),
            base: toml::from_str(base_toml).unwrap(),
        }
    }

    #[test]
    fn saves_value_that_differs_from_base() {
        let dir = tempfile::tempdir().unwrap();
        let disk = make_disk(dir.path(), "[network]\nrender_distance = 8\n");

        save_config_section(&disk, &NetCfg { render_distance: 16, extra: String::new() })
            .unwrap();

        let written = std::fs::read_to_string(disk.path).unwrap();
        let table: toml::Table = toml::from_str(&written).unwrap();
        assert_eq!(table["network"]["render_distance"].as_integer(), Some(16));
    }

    #[test]
    fn omits_value_matching_base_when_not_in_target() {
        let dir = tempfile::tempdir().unwrap();
        let disk = make_disk(dir.path(), "[network]\nrender_distance = 8\n");

        // Save with render_distance == base value (8).
        save_config_section(&disk, &NetCfg { render_distance: 8, extra: String::new() })
            .unwrap();

        let written = std::fs::read_to_string(disk.path).unwrap();
        let table: toml::Table = toml::from_str(&written).unwrap();
        // Should not be written since it matches the base.
        assert!(!table
            .get("network")
            .and_then(|t| t.as_table())
            .map(|t| t.contains_key("render_distance"))
            .unwrap_or(false));
    }

    #[test]
    fn updates_key_already_in_target_even_if_matching_base() {
        let dir = tempfile::tempdir().unwrap();
        // Pre-populate the target file with render_distance = 8 (same as base).
        let path = dir.path().join("config.toml");
        std::fs::write(&path, b"[network]\nrender_distance = 8\n").unwrap();

        let disk = make_disk(dir.path(), "[network]\nrender_distance = 8\n");
        // Save same value — since it's already in the target, it should be updated (kept).
        save_config_section(&disk, &NetCfg { render_distance: 8, extra: String::new() })
            .unwrap();

        let written = std::fs::read_to_string(disk.path).unwrap();
        let table: toml::Table = toml::from_str(&written).unwrap();
        assert_eq!(table["network"]["render_distance"].as_integer(), Some(8));
    }

    #[test]
    fn preserves_unknown_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            b"[my_mod]\nspawn_rate = 0.5\n[network]\nrender_distance = 4\n",
        )
        .unwrap();

        let disk = make_disk(dir.path(), "");
        save_config_section(&disk, &NetCfg { render_distance: 32, extra: String::new() })
            .unwrap();

        let written = std::fs::read_to_string(disk.path).unwrap();
        let table: toml::Table = toml::from_str(&written).unwrap();
        assert!(table.contains_key("my_mod"), "unknown section must survive");
        assert_eq!(
            table["my_mod"]["spawn_rate"].as_float(),
            Some(0.5),
            "unknown section value must survive"
        );
    }

    #[test]
    fn preserves_unknown_keys_in_updated_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        // "future_key" is not in NetCfg — simulate a key from a newer config version.
        std::fs::write(
            &path,
            b"[network]\nrender_distance = 4\nfuture_key = \"keep_me\"\n",
        )
        .unwrap();

        let disk = make_disk(dir.path(), "");
        save_config_section(&disk, &NetCfg { render_distance: 8, extra: String::new() })
            .unwrap();

        let written = std::fs::read_to_string(disk.path).unwrap();
        let table: toml::Table = toml::from_str(&written).unwrap();
        assert_eq!(
            table["network"]["future_key"].as_str(),
            Some("keep_me"),
            "unknown intra-section key must survive"
        );
    }

    #[test]
    fn atomic_write_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let disk = make_disk(dir.path(), "");
        save_config_section(&disk, &NetCfg { render_distance: 5, extra: String::new() })
            .unwrap();
        let tmp = disk.path.with_extension("toml.tmp");
        assert!(!tmp.exists(), "temp file must be cleaned up");
    }
}
