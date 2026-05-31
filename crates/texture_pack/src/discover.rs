//! Filesystem discovery of Minecraft-style texture-pack assets.
//!
//! [`discover`] walks one or more pack-root directories, finds every
//! `assets/<namespace>/textures/block/**/*.png` file, derives the
//! canonical `"<namespace>:block/<path>"` key, and applies the
//! "later paths win" override rule.  The result is a
//! [`Vec<DiscoveredTexture>`].
//!
//! This stage does **not** read PNG pixels or parse `.mcmeta` — it
//! only discovers files.  Decoding happens in a later stage so the
//! discovery layer can stay pure-data and easy to test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// One texture located on disk during discovery.
///
/// `key` is the Minecraft-style identifier the rest of the engine
/// uses (e.g. `"minecraft:block/stone"`).  `png_path` and
/// `mcmeta_path` are absolute, ready to be passed to the decode
/// stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredTexture {
    /// `"<namespace>:block/<relative_path_without_ext>"`.
    pub key: String,
    /// Absolute path to the PNG file.
    pub png_path: PathBuf,
    /// Absolute path to the companion `.png.mcmeta`, if it exists on
    /// disk.  Discovery does not parse the file; it merely notes its
    /// presence.
    pub mcmeta_path: Option<PathBuf>,
    /// The pack-root directory this texture came from.  Recorded so
    /// log lines can identify which pack "won" an override.
    pub source_pack: PathBuf,
}

/// Walks every search path and returns the resolved texture set.
///
/// # Behaviour
///
/// - For each pack root, finds every regular file matching
///   `assets/<ns>/textures/block/**/*.png`.
/// - Computes the key `"<ns>:block/<rel_path_without_ext>"`.
/// - When the same key appears in more than one pack, **the later
///   pack root in `search_paths` wins**.
/// - Missing pack-root directories are silently skipped (with a
///   debug-level log line at a higher layer; not logged here so the
///   function stays pure for tests).
///
/// The returned vector is sorted by key for stable iteration.
pub fn discover(search_paths: &[PathBuf]) -> Vec<DiscoveredTexture> {
    let mut by_key: BTreeMap<String, DiscoveredTexture> = BTreeMap::new();

    for pack_root in search_paths {
        if !pack_root.is_dir() {
            continue;
        }
        let assets_dir = pack_root.join("assets");
        if !assets_dir.is_dir() {
            continue;
        }
        for ns_entry in WalkDir::new(&assets_dir).min_depth(1).max_depth(1) {
            let Ok(ns_entry) = ns_entry else { continue };
            if !ns_entry.file_type().is_dir() {
                continue;
            }
            let namespace = ns_entry.file_name().to_string_lossy().to_string();
            let block_root = ns_entry.path().join("textures").join("block");
            if !block_root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(&block_root).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let png_path = entry.path();
                if png_path.extension().and_then(|s| s.to_str()) != Some("png") {
                    continue;
                }
                let Some(rel) = png_path
                    .strip_prefix(&block_root)
                    .ok()
                    .and_then(strip_png_ext)
                else {
                    continue;
                };
                let key = format!("{namespace}:block/{rel}");
                let mcmeta_path = png_path.with_extension("png.mcmeta");
                let mcmeta_path = mcmeta_path.is_file().then_some(mcmeta_path);
                by_key.insert(
                    key.clone(),
                    DiscoveredTexture {
                        key,
                        png_path: png_path.to_path_buf(),
                        mcmeta_path,
                        source_pack: pack_root.clone(),
                    },
                );
            }
        }
    }

    by_key.into_values().collect()
}

fn strip_png_ext(rel: &Path) -> Option<String> {
    let parent = rel.parent()?;
    let stem = rel.file_stem()?.to_str()?;
    let joined = if parent.as_os_str().is_empty() {
        stem.to_owned()
    } else {
        format!("{}/{stem}", parent.to_string_lossy().replace('\\', "/"))
    };
    Some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn discovers_single_pack_textures() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(&root.join("assets/minecraft/textures/block/stone.png"));
        touch(&root.join("assets/minecraft/textures/block/oak_log.png"));
        touch(&root.join("assets/minecraft/textures/block/nested/deep_slate.png"));
        // Non-block textures must be ignored.
        touch(&root.join("assets/minecraft/textures/item/apple.png"));
        // Non-PNG files must be ignored.
        touch(&root.join("assets/minecraft/textures/block/readme.txt"));

        let found = discover(&[root.to_path_buf()]);
        let keys: Vec<&str> = found.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "minecraft:block/nested/deep_slate",
                "minecraft:block/oak_log",
                "minecraft:block/stone",
            ]
        );
    }

    #[test]
    fn later_pack_overrides_earlier_pack() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("base");
        let user = tmp.path().join("user");

        touch(&base.join("assets/minecraft/textures/block/stone.png"));
        touch(&user.join("assets/minecraft/textures/block/stone.png"));

        let found = discover(&[base.clone(), user.clone()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_pack, user);
    }

    #[test]
    fn notes_mcmeta_when_present() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let png = root.join("assets/minecraft/textures/block/water_flow.png");
        touch(&png);
        let mcmeta = root.join("assets/minecraft/textures/block/water_flow.png.mcmeta");
        touch(&mcmeta);

        let found = discover(&[root.to_path_buf()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mcmeta_path.as_deref(), Some(mcmeta.as_path()));
    }

    #[test]
    fn missing_search_path_is_silently_skipped() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().to_path_buf();
        touch(&real.join("assets/minecraft/textures/block/stone.png"));
        let missing = tmp.path().join("does-not-exist");

        let found = discover(&[missing, real]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "minecraft:block/stone");
    }

    #[test]
    fn empty_search_paths_yields_empty_result() {
        assert!(discover(&[]).is_empty());
    }

    #[test]
    fn multiple_namespaces_are_kept_distinct() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(&root.join("assets/minecraft/textures/block/stone.png"));
        touch(&root.join("assets/dd40/textures/block/stone.png"));

        let found = discover(&[root.to_path_buf()]);
        let keys: Vec<&str> = found.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["dd40:block/stone", "minecraft:block/stone"],
            "namespaces must coexist as distinct keys"
        );
    }
}
