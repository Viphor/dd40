//! Minecraft-compatible texture-pack loader for dd40.
//!
//! # Overview
//!
//! `dd40_texture_pack` is the Tier-1 implementation that turns one or
//! more directories of Minecraft-style resource-pack assets into a
//! populated [`BlockAtlas`].  It is the default consumer of the
//! [`dd40_texture_core`] vocabulary — but not the only possible one:
//! anyone may write a different atlas-owning plugin (e.g. for a
//! hand-built atlas, or a streaming pack format) and skip this crate
//! entirely.
//!
//! # Conventions
//!
//! - Search paths are scanned for `assets/<ns>/textures/block/**/*.png`.
//! - Each PNG becomes one named texture, keyed `"<ns>:block/<path>"`,
//!   matching how Minecraft and `BlockTextures::all(...)` reference
//!   textures.
//! - A companion `<file>.png.mcmeta` JSON file declares animation
//!   frames.
//! - When a key appears in more than one search path, **later paths
//!   override earlier ones** (last-write-wins) so a user pack at the
//!   end of the search list can shadow the default pack.
//!
//! # Build pipeline
//!
//! 1. [`discover`] walks every search path and produces a list of
//!    [`DiscoveredTexture`]s with the override rules applied.
//! 2. (Future commit.) A decode stage loads each PNG, parses the
//!    `.mcmeta`, classifies the render layer by alpha histogram,
//!    and resizes frames to a uniform size.
//! 3. (Future commit.) An upload stage writes the frames into a
//!    `bevy::image::Image` 2D array texture, builds a
//!    `BlockAtlasSource`, and installs it on the
//!    [`BlockAtlas`](dd40_texture_core::BlockAtlas) resource in the
//!    [`AtlasReady`](dd40_texture_core::AtlasReady) system set.
//!
//! Each stage is exposed as its own pure-data function so it can be
//! unit-tested without spinning up a Bevy app or a GPU.
//!
//! # Usage
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_texture_pack::prelude::*;
//!
//! App::new()
//!     .insert_resource(TexturePackConfig::with_search_path("assets/resourcepacks/default"))
//!     .add_plugins(TexturePackPlugin)
//!     .run();
//! ```

pub mod config;
pub mod discover;
pub mod plugin;
pub mod prelude;

pub use config::TexturePackConfig;
pub use discover::{DiscoveredTexture, discover};
pub use plugin::TexturePackPlugin;
