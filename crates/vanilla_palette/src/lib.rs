//! Vanilla game content for dd40.
//!
//! This crate defines all "out of the box" content: blocks, tool kinds, and
//! tool tiers.  It is intentionally separate from [`dd40_core`] so that
//! modders who want a completely different set of blocks and tools can depend
//! only on `dd40_core` and ignore this crate entirely.
//!
//! # Usage
//!
//! Add [`VanillaPalettePlugin`] to your app alongside [`CorePlugin`]:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_core::plugin::CorePlugin;
//! use dd40_vanilla_palette::VanillaPalettePlugin;
//!
//! App::new()
//!     .add_plugins((CorePlugin, VanillaPalettePlugin))
//!     .run();
//! ```
//!
//! The plugin registers all vanilla tool kinds and tiers (in [`ToolRegistrySet`])
//! and all vanilla blocks (in [`BlockRegistrySet`]).
//!
//! # Future content
//!
//! Mob and animal definitions will also live here once the entity system
//! supports them.
//!
//! [`CorePlugin`]: dd40_core::plugin::CorePlugin
//! [`ToolRegistrySet`]: dd40_core::tools::ToolRegistrySet
//! [`BlockRegistrySet`]: dd40_core::block::registry::BlockRegistrySet

use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;

pub mod blocks;
pub mod tools;

pub use blocks::{VanillaBlocks, VanillaBlocksPlugin};
pub use tools::{VanillaToolKinds, VanillaToolTiers, VanillaToolsPlugin};

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers all vanilla blocks, tool kinds, and tool tiers.
///
/// This plugin must be added **after** [`CorePlugin`] (which inserts
/// [`BlockRegistry`] and [`ToolRegistry`]).
///
/// Internally it composes [`VanillaToolsPlugin`] (runs in [`ToolRegistrySet`])
/// and [`VanillaBlocksPlugin`] (runs in [`BlockRegistrySet`]).  The system-set
/// ordering `ToolRegistrySet → BlockRegistrySet` is already configured by
/// `CorePlugin`, so vanilla tool kinds are guaranteed to exist when vanilla
/// block definitions reference them via `preferred_tool`.
///
/// [`CorePlugin`]: dd40_core::plugin::CorePlugin
/// [`BlockRegistry`]: dd40_core::block::BlockRegistry
/// [`ToolRegistry`]: dd40_core::tools::ToolRegistry
/// [`ToolRegistrySet`]: dd40_core::tools::ToolRegistrySet
/// [`BlockRegistrySet`]: dd40_core::block::registry::BlockRegistrySet
pub struct VanillaPalettePlugin;

impl Plugin for VanillaPalettePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);
        app.add_plugins((VanillaToolsPlugin, VanillaBlocksPlugin));
    }
}
