//! The [`ConfigSection`] trait — the only contract downstream crates implement.

/// A TOML top-level section that can be loaded from [`crate::RawConfig`].
///
/// Implement this on your config struct to read from the shared `config.toml`.
/// The `SECTION` constant must match the TOML table key and the middle segment
/// of the `DD40_<SECTION>__<KEY>` env var prefix:
///
/// | Crate             | Struct              | `SECTION`       |
/// |-------------------|---------------------|-----------------|
/// | `dd40_network`    | `NetworkConfig`     | `"network"`     |
/// | `dd40_chunk_storage` | `ChunkStorageConfig` | `"chunk_storage"` |
/// | `dd40_texture_pack` | `TexturePackConfig` | `"texture_pack"` |
///
/// # Example
///
/// ```rust
/// use dd40_config::ConfigSection;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Deserialize, Serialize)]
/// #[serde(default)]
/// pub struct NetworkConfig {
///     pub render_distance: i32,
/// }
///
/// impl Default for NetworkConfig {
///     fn default() -> Self { Self { render_distance: 8 } }
/// }
///
/// impl ConfigSection for NetworkConfig {
///     const SECTION: &'static str = "network";
/// }
/// ```
pub trait ConfigSection: serde::de::DeserializeOwned + serde::Serialize + Default {
    /// The top-level TOML table key for this section (e.g. `"network"`).
    const SECTION: &'static str;
}
