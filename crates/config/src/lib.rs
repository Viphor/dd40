//! TOML-based configuration system for dd40.
//!
//! # Overview
//!
//! This crate provides an **open, extensible** config system. Any crate —
//! including third-party mods — can read its own section from the config file
//! by implementing [`ConfigSection`]. `dd40_config` itself never enumerates
//! known section names.
//!
//! # Quick start
//!
//! ## 1. Implement `ConfigSection` on your config struct
//!
//! ```rust
//! use dd40_config::ConfigSection;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Deserialize, Serialize)]
//! #[serde(default)]
//! pub struct NetworkConfig {
//!     pub render_distance: i32,
//! }
//!
//! impl Default for NetworkConfig {
//!     fn default() -> Self { Self { render_distance: 8 } }
//! }
//!
//! impl ConfigSection for NetworkConfig {
//!     const SECTION: &'static str = "network";
//! }
//! ```
//!
//! ## 2. Add `ConfigPlugin` before other dd40 plugins
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use dd40_config::ConfigPlugin;
//!
//! App::new().add_plugins(ConfigPlugin).run();
//! ```
//!
//! ## 3. Read your section in a `Startup` system
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use dd40_config::{ConfigSection, RawConfig};
//! # use serde::{Deserialize, Serialize};
//! # #[derive(Debug, Clone, Deserialize, Serialize, Default, Resource)]
//! # struct NetworkConfig { render_distance: i32 }
//! # impl ConfigSection for NetworkConfig { const SECTION: &'static str = "network"; }
//! fn init(raw: Res<RawConfig>, mut commands: Commands) {
//!     commands.insert_resource(raw.section::<NetworkConfig>());
//! }
//! ```
//!
//! ## 4. Save changes back to disk
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use dd40_config::{ConfigSection, ConfigDisk, save_config_section};
//! # use serde::{Deserialize, Serialize};
//! # #[derive(Debug, Clone, Deserialize, Serialize, Default, Resource)]
//! # struct NetworkConfig { render_distance: i32 }
//! # impl ConfigSection for NetworkConfig { const SECTION: &'static str = "network"; }
//! fn on_save(disk: Res<ConfigDisk>, cfg: Res<NetworkConfig>) {
//!     if let Err(e) = save_config_section(&disk, &*cfg) {
//!         warn!("could not save config: {e}");
//!     }
//! }
//! ```
//!
//! # Config file locations (lowest → highest priority)
//!
//! 1. Compiled-in [`Default`] on each section struct.
//! 2. Platform config dir (`~/.config/dd40/config.toml` on Linux/macOS).
//! 3. Binary-adjacent `./config.toml`.
//! 4. Path from `DD40_CONFIG` env var.
//! 5. Per-key env var overrides: `DD40_<SECTION>__<KEY>=<VALUE>`.
//!
//! # Env var overrides
//!
//! Any `DD40_<SECTION>__<KEY>` env var overrides the corresponding key for all
//! sections. Values are auto-coerced to bool, integer, float, or string.

pub mod disk;
pub mod env;
pub mod load;
pub mod plugin;
pub mod raw;
pub mod register;
pub mod section;

pub use disk::{ConfigDisk, ConfigSaveError, save_config_section};
pub use plugin::ConfigPlugin;
pub use raw::RawConfig;
pub use register::RegisterConfig;
pub use section::ConfigSection;
