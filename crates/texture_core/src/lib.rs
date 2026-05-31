//! Foundation vocabulary for the dd40 texture system.
//!
//! # Overview
//!
//! `dd40_texture_core` defines the **types** every texture-aware crate
//! shares: how a block points at one or more textures, how a resolved
//! atlas entry is described, and how the runtime atlas resource is
//! looked up.  It owns **no behaviour** — it does not load PNGs, it
//! does not build a GPU atlas, and it does not render anything.  Those
//! concerns live in [`dd40_texture_pack`](../texture_pack) (the
//! Minecraft-compatible loader) and `dd40_renderer` (the consumer).
//!
//! # Why this is a separate crate
//!
//! Texturing is **opt-in**: a 2D, text-mode, or custom-renderer dd40
//! build must compile without ever knowing textures exist.  Therefore
//! these types must not live in `dd40_core` — that would force every
//! downstream crate to depend on them.  At the same time, more than
//! one Tier-1 implementation crate needs to speak this vocabulary
//! (`dd40_renderer`, `dd40_texture_pack`, `dd40_vanilla_palette`,
//! `dd40_inventory_gui`, `dd40_loose_item_render`), and Tier-1 crates
//! may not depend on each other.  The three-tier architecture's
//! resolution is exactly this crate: a Tier-0 foundation containing
//! only the shared types.
//!
//! # Attaching textures to blocks
//!
//! Textures are not a built-in field on [`BlockDefinition`] — they
//! ride on the existing typed [`BlockData`] extension mechanism, the
//! same hook used by `LootTable` today:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_core::block::{BlockDefinition, BlockId};
//! use dd40_texture_core::prelude::*;
//!
//! let def = BlockDefinition::new(BlockId(1000), "copper_ore")
//!     .with_data(BlockTextures::all(TextureRef::named("dd40:block/copper_ore")));
//! ```
//!
//! For asymmetric blocks each face can specify its own texture:
//!
//! ```no_run
//! use dd40_core::block::{BlockDefinition, BlockId};
//! use dd40_texture_core::prelude::*;
//!
//! let def = BlockDefinition::new(BlockId(1001), "log")
//!     .with_data(
//!         BlockTextures::top_bottom_sides(
//!             TextureRef::named("minecraft:block/oak_log_top"),
//!             TextureRef::named("minecraft:block/oak_log_top"),
//!             TextureRef::named("minecraft:block/oak_log"),
//!         ),
//!     );
//! ```
//!
//! # Resolving textures at runtime
//!
//! At runtime, an atlas-owning plugin (the texture-pack loader, or a
//! user-written equivalent) inserts a value implementing
//! [`BlockAtlas`] into the world.  Consumers (the renderer, the
//! inventory icon cache) query it via [`BlockAtlas::resolve`].
//!
//! # Usage
//!
//! Add [`TextureCorePlugin`] once.  It registers [`BlockTextures`] and
//! [`RenderLayer`] with the [`BlockDataTypeRegistry`] so they can ride
//! through cell-data serialisation like any other `BlockData`.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_texture_core::prelude::*;
//!
//! App::new().add_plugins(TextureCorePlugin).run();
//! ```
//!
//! [`BlockData`]: dd40_core::block::BlockData
//! [`BlockDataTypeRegistry`]: dd40_core::block::BlockDataTypeRegistry
//! [`BlockDefinition`]: dd40_core::block::BlockDefinition

pub mod animation;
pub mod atlas;
pub mod block_textures;
pub mod plugin;
pub mod prelude;
pub mod render_layer;
pub mod texture_ref;

pub use animation::AnimationSpec;
pub use atlas::{AtlasId, AtlasReady, AtlasUv, BlockAtlas, BlockAtlasSource, ResolvedTexture};
pub use block_textures::{BlockTextures, Face};
pub use plugin::TextureCorePlugin;
pub use render_layer::RenderLayer;
pub use texture_ref::TextureRef;
