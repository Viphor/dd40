//! Vanilla tool kinds and tiers.
//!
//! Provides [`VanillaToolsPlugin`] (registers in [`ToolRegistrySet`]) and the
//! constant structs [`VanillaToolKinds`] / [`VanillaToolTiers`] for convenient
//! access to vanilla tool IDs elsewhere.
//!
//! [`ToolRegistrySet`]: dd40_core::tools::ToolRegistrySet

use bevy::prelude::*;
use dd40_core::tools::{ToolKindDefinition, ToolKindId, ToolRegistry, ToolRegistrySet, ToolTierDefinition, ToolTierId};

// ── Constants ─────────────────────────────────────────────────────────────────

/// [`ToolKindId`] constants for the vanilla tool kinds.
///
/// ID `0` is reserved by the engine for [`ToolKindId::NONE`] (bare hands).
/// Vanilla kinds start at `1`.
pub struct VanillaToolKinds;

impl VanillaToolKinds {
    /// Bare hands — the engine invariant; no preferred-tool match.
    pub const HAND: ToolKindId = ToolKindId(0);
    /// Pickaxe — preferred for stone, ores, and metal blocks.
    pub const PICKAXE: ToolKindId = ToolKindId(1);
    /// Axe — preferred for wood and wooden blocks.
    pub const AXE: ToolKindId = ToolKindId(2);
    /// Shovel — preferred for dirt, sand, and gravel.
    pub const SHOVEL: ToolKindId = ToolKindId(3);
    /// Hoe — preferred for farmland and crop blocks.
    pub const HOE: ToolKindId = ToolKindId(4);
    /// Shears — preferred for leaves and wool.
    pub const SHEARS: ToolKindId = ToolKindId(5);
}

/// [`ToolTierId`] constants for the vanilla tool tiers.
///
/// ID `0` is reserved by the engine for [`ToolTierId::DEFAULT`] (bare hands /
/// no tier bonus).  Vanilla tiers start at `1`.
pub struct VanillaToolTiers;

impl VanillaToolTiers {
    /// Bare hands — no speed bonus (`speed_multiplier = 1.0`).
    pub const HAND: ToolTierId = ToolTierId(0);
    /// Wood — `speed_multiplier = 2.0`.
    pub const WOOD: ToolTierId = ToolTierId(1);
    /// Stone — `speed_multiplier = 4.0`.
    pub const STONE: ToolTierId = ToolTierId(2);
    /// Iron — `speed_multiplier = 6.0`.
    pub const IRON: ToolTierId = ToolTierId(3);
    /// Diamond — `speed_multiplier = 8.0`.
    pub const DIAMOND: ToolTierId = ToolTierId(4);
    /// Gold — `speed_multiplier = 12.0` (fast but fragile).
    pub const GOLD: ToolTierId = ToolTierId(5);
}

// ── Registration system ───────────────────────────────────────────────────────

fn register_vanilla_tools(mut registry: ResMut<ToolRegistry>) {
    // Kinds — ID 0 is the engine "NONE" sentinel, registered by ToolRegistry::new().
    // We overlay it with a human-readable name here.
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::HAND, "hand"));
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::PICKAXE, "pickaxe"));
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::AXE, "axe"));
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::SHOVEL, "shovel"));
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::HOE, "hoe"));
    registry.register_kind(ToolKindDefinition::new(VanillaToolKinds::SHEARS, "shears"));

    // Tiers — ID 0 is the engine "DEFAULT" sentinel, overlaid with "hand".
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::HAND, "hand", 1.0));
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::WOOD, "wood", 2.0));
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::STONE, "stone", 4.0));
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::IRON, "iron", 6.0));
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::DIAMOND, "diamond", 8.0));
    registry.register_tier(ToolTierDefinition::new(VanillaToolTiers::GOLD, "gold", 12.0));
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers all vanilla tool kinds and tiers during [`ToolRegistrySet`].
///
/// Added automatically by [`VanillaPalettePlugin`]; you can also add it directly
/// if you only want the vanilla tool definitions without the vanilla blocks.
///
/// [`VanillaPalettePlugin`]: crate::VanillaPalettePlugin
pub struct VanillaToolsPlugin;

impl Plugin for VanillaToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_vanilla_tools.in_set(ToolRegistrySet));
    }
}
