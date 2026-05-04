//! Tool registry — kinds, tiers, and the mining-duration formula.
//!
//! This module is purely engine vocabulary: it defines the *types* needed to
//! represent and query tools, but registers **no vanilla content**.  Vanilla
//! tool kinds and tiers (Pickaxe, Iron, etc.) live in `dd40_vanilla_palette`.
//!
//! # Registry model
//!
//! Both tool kinds and tool tiers are stored in [`ToolRegistry`] and addressed
//! by opaque integer IDs ([`ToolKindId`] / [`ToolTierId`]).  The pattern is
//! intentionally identical to the block registry so modders can extend the tool
//! system without touching core.
//!
//! # Engine invariants
//!
//! - [`ToolKindId::NONE`] (`0`) — "no tool / bare hands".  Blocks whose
//!   `preferred_tool` is `Some(x)` will never match this, so no speed bonus
//!   is applied when bare-handed.
//! - [`ToolTierId::DEFAULT`] (`0`) — the lowest tier, `speed_multiplier = 1.0`.
//!
//! # System ordering
//!
//! Tool-registration systems must be placed in [`ToolRegistrySet`], which is
//! configured to run **before** [`BlockRegistrySet`] during `Startup`.  This
//! guarantees that [`ToolKindId`] values are valid when block definitions
//! reference them via `preferred_tool`.
//!
//! [`BlockRegistrySet`]: crate::block::registry::BlockRegistrySet

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::block::BlockDefinition;

// ── System set ────────────────────────────────────────────────────────────────

/// System set for tool-registration systems.
///
/// Add your registration systems here during `Startup`.  This set is
/// configured to run **before** [`BlockRegistrySet`] so that block definitions
/// can safely reference [`ToolKindId`] values.
///
/// [`BlockRegistrySet`]: crate::block::registry::BlockRegistrySet
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolRegistrySet;

// ── IDs ───────────────────────────────────────────────────────────────────────

/// Opaque index into the [`ToolRegistry`]'s kind table.
///
/// ID `0` is the engine invariant "no tool / bare hands" ([`ToolKindId::NONE`]).
/// Vanilla kinds start at `1` (registered by `dd40_vanilla_palette`).
/// Custom modded kinds should start at `64` or higher to leave room for future
/// vanilla additions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize, Default)]
pub struct ToolKindId(pub u16);

impl ToolKindId {
    /// The "no tool" sentinel: bare hands.  No speed bonus is ever awarded when
    /// this kind is equipped, because block `preferred_tool` values are always
    /// `Some(non-zero)` or `None`.
    pub const NONE: Self = Self(0);
}

impl std::fmt::Display for ToolKindId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolKindId({})", self.0)
    }
}

/// Opaque index into the [`ToolRegistry`]'s tier table.
///
/// ID `0` is the engine invariant default tier ([`ToolTierId::DEFAULT`]),
/// with `speed_multiplier = 1.0`.  Vanilla tiers are registered by
/// `dd40_vanilla_palette`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize, Default)]
pub struct ToolTierId(pub u16);

impl ToolTierId {
    /// The default tier (bare hands / no tool).  Its `speed_multiplier` is
    /// `1.0`, meaning no speed bonus over the block's base toughness.
    pub const DEFAULT: Self = Self(0);
}

impl std::fmt::Display for ToolTierId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolTierId({})", self.0)
    }
}

// ── Definitions ───────────────────────────────────────────────────────────────

/// The definition of a tool *kind* (category).
///
/// A kind is a named category like "Pickaxe" or "Axe".  It has no mechanical
/// properties of its own — speed bonuses come from the [`ToolTierDefinition`].
#[derive(Debug, Clone, Reflect)]
pub struct ToolKindDefinition {
    /// Unique identifier for this tool kind.
    pub id: ToolKindId,
    /// Human-readable name (e.g. `"pickaxe"`).
    pub name: String,
}

impl ToolKindDefinition {
    /// Creates a new tool kind definition.
    pub fn new(id: ToolKindId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

/// The definition of a tool *tier* (material quality).
///
/// A tier carries the `speed_multiplier` that is applied when the player uses
/// a tool of the matching kind against a block that lists that kind as its
/// `preferred_tool`.
///
/// # Speed formula
///
/// `mining_duration = block.toughness / tier.speed_multiplier`
///
/// A multiplier of `1.0` means no bonus (bare hands / wrong kind).
#[derive(Debug, Clone, Reflect)]
pub struct ToolTierDefinition {
    /// Unique identifier for this tier.
    pub id: ToolTierId,
    /// Human-readable name (e.g. `"iron"`).
    pub name: String,
    /// How much faster this tier mines a block compared to bare hands.
    ///
    /// `1.0` = no bonus.  Values above `1.0` reduce mining time.
    /// The bare-hands / default tier must have `speed_multiplier = 1.0`.
    pub speed_multiplier: f32,
}

impl ToolTierDefinition {
    /// Creates a new tool tier definition.
    pub fn new(id: ToolTierId, name: impl Into<String>, speed_multiplier: f32) -> Self {
        Self {
            id,
            name: name.into(),
            speed_multiplier,
        }
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Registry that stores all registered tool kinds and tiers.
///
/// This resource is inserted by [`CorePlugin`] with the engine-invariant
/// defaults pre-populated.  Vanilla kinds and tiers are registered by
/// `dd40_vanilla_palette`; modded ones may be registered by any crate in
/// [`ToolRegistrySet`].
///
/// [`CorePlugin`]: crate::plugin::CorePlugin
#[derive(Resource, Default, Reflect)]
pub struct ToolRegistry {
    kinds: Vec<ToolKindDefinition>,
    tiers: Vec<ToolTierDefinition>,
}

impl ToolRegistry {
    /// Creates a new registry with the engine-invariant defaults pre-registered.
    ///
    /// - Kind `0` = "none" (bare hands sentinel)
    /// - Tier `0` = "default" (`speed_multiplier = 1.0`)
    pub fn new() -> Self {
        let mut registry = Self {
            kinds: Vec::new(),
            tiers: Vec::new(),
        };

        registry.register_kind(ToolKindDefinition::new(ToolKindId::NONE, "none"));
        registry.register_tier(ToolTierDefinition::new(ToolTierId::DEFAULT, "default", 1.0));

        registry
    }

    /// Registers a new tool kind.
    ///
    /// If the slot at `definition.id` is already occupied it will be replaced.
    /// IDs are contiguous — gaps are filled with placeholder "unknown" kinds.
    ///
    /// Returns the assigned [`ToolKindId`].
    pub fn register_kind(&mut self, definition: ToolKindDefinition) -> ToolKindId {
        let id = definition.id;
        while self.kinds.len() <= id.0 as usize {
            let placeholder_id = ToolKindId(self.kinds.len() as u16);
            self.kinds.push(ToolKindDefinition::new(
                placeholder_id,
                format!("unknown_kind_{}", placeholder_id.0),
            ));
        }
        self.kinds[id.0 as usize] = definition;
        id
    }

    /// Registers a new tool tier.
    ///
    /// If the slot at `definition.id` is already occupied it will be replaced.
    /// IDs are contiguous — gaps are filled with placeholder tiers (`multiplier = 1.0`).
    ///
    /// Returns the assigned [`ToolTierId`].
    pub fn register_tier(&mut self, definition: ToolTierDefinition) -> ToolTierId {
        let id = definition.id;
        while self.tiers.len() <= id.0 as usize {
            let placeholder_id = ToolTierId(self.tiers.len() as u16);
            self.tiers.push(ToolTierDefinition::new(
                placeholder_id,
                format!("unknown_tier_{}", placeholder_id.0),
                1.0,
            ));
        }
        self.tiers[id.0 as usize] = definition;
        id
    }

    /// Returns the kind definition for the given ID, or `None` if unregistered.
    pub fn get_kind(&self, id: ToolKindId) -> Option<&ToolKindDefinition> {
        self.kinds.get(id.0 as usize)
    }

    /// Returns the tier definition for the given ID, or `None` if unregistered.
    pub fn get_tier(&self, id: ToolTierId) -> Option<&ToolTierDefinition> {
        self.tiers.get(id.0 as usize)
    }

    /// Returns the speed multiplier for the given tier.
    ///
    /// Falls back to `1.0` for unregistered tiers (no bonus).
    pub fn speed_multiplier(&self, tier: ToolTierId) -> f32 {
        self.get_tier(tier)
            .map(|def| def.speed_multiplier)
            .unwrap_or(1.0)
    }

    /// Returns an iterator over all registered kinds.
    pub fn iter_kinds(&self) -> impl Iterator<Item = &ToolKindDefinition> {
        self.kinds.iter()
    }

    /// Returns an iterator over all registered tiers.
    pub fn iter_tiers(&self) -> impl Iterator<Item = &ToolTierDefinition> {
        self.tiers.iter()
    }
}

// ── Mining duration formula ───────────────────────────────────────────────────

/// Computes how many seconds the player needs to hold left-click to mine
/// `block_def` with a tool of the given kind and tier.
///
/// Returns `None` if the block is not destructible (e.g. bedrock).
/// Returns `Some(0.0)` if the block is instant-mine (`toughness ≤ 0.0`).
///
/// # Speed rule
///
/// A speed bonus is applied only when **both** conditions hold:
/// - `block_def.preferred_tool == Some(tool_kind)`
/// - `tool_kind != ToolKindId::NONE`
///
/// Otherwise the tier multiplier is ignored and the player mines at bare-hand
/// speed (multiplier `1.0`), regardless of tier.
///
/// # Examples
///
/// ```
/// use dd40_core::tools::{ToolRegistry, ToolKindId, ToolTierId, mining_duration};
/// use dd40_core::block::registry::BlockDefinition;
///
/// let tool_registry = ToolRegistry::new();
/// // Bare hands: kind = NONE, tier = DEFAULT
/// let block = BlockDefinition::new(dd40_core::block::BlockId(1), "stone")
///     .with_toughness(1.5);
/// assert_eq!(
///     mining_duration(&block, ToolKindId::NONE, ToolTierId::DEFAULT, &tool_registry),
///     Some(1.5),
/// );
/// ```
pub fn mining_duration(
    block_def: &BlockDefinition,
    tool_kind: ToolKindId,
    tool_tier: ToolTierId,
    tool_registry: &ToolRegistry,
) -> Option<f32> {
    if !block_def.is_destructible {
        return None;
    }

    if block_def.toughness <= 0.0 {
        return Some(0.0);
    }

    let multiplier = match block_def.preferred_tool {
        Some(preferred) if preferred == tool_kind && tool_kind != ToolKindId::NONE => {
            tool_registry.speed_multiplier(tool_tier)
        }
        _ => 1.0,
    };

    Some(block_def.toughness / multiplier)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockId, registry::BlockDefinition};

    fn make_registry_with_iron_pickaxe() -> (ToolRegistry, ToolKindId, ToolTierId) {
        let mut registry = ToolRegistry::new();
        let pickaxe = registry.register_kind(ToolKindDefinition::new(ToolKindId(1), "pickaxe"));
        let iron = registry.register_tier(ToolTierDefinition::new(ToolTierId(1), "iron", 6.0));
        (registry, pickaxe, iron)
    }

    #[test]
    fn default_tier_has_multiplier_one() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.speed_multiplier(ToolTierId::DEFAULT), 1.0);
    }

    #[test]
    fn bare_hands_no_bonus() {
        let (registry, pickaxe, _) = make_registry_with_iron_pickaxe();
        let block = BlockDefinition::new(BlockId(1), "stone")
            .with_toughness(1.5)
            .with_preferred_tool(pickaxe);
        // NONE kind != preferred (pickaxe), so multiplier = 1.0
        assert_eq!(
            mining_duration(&block, ToolKindId::NONE, ToolTierId::DEFAULT, &registry),
            Some(1.5),
        );
    }

    #[test]
    fn correct_tool_applies_multiplier() {
        let (registry, pickaxe, iron) = make_registry_with_iron_pickaxe();
        let block = BlockDefinition::new(BlockId(1), "stone")
            .with_toughness(1.5)
            .with_preferred_tool(pickaxe);
        // 1.5 / 6.0 = 0.25 s
        assert!((mining_duration(&block, pickaxe, iron, &registry).unwrap() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn wrong_tool_kind_no_bonus() {
        let (registry, pickaxe, iron) = make_registry_with_iron_pickaxe();
        let block = BlockDefinition::new(BlockId(1), "stone")
            .with_toughness(1.5)
            .with_preferred_tool(pickaxe);
        // Equip an unregistered kind (treated as wrong) with iron tier:
        // kind != preferred so multiplier = 1.0.
        assert_eq!(
            mining_duration(&block, ToolKindId(2), iron, &registry),
            Some(1.5),
        );
    }

    #[test]
    fn indestructible_returns_none() {
        let (registry, _, iron) = make_registry_with_iron_pickaxe();
        let block = BlockDefinition::new(BlockId(99), "bedrock").with_destructible(false);
        assert_eq!(
            mining_duration(&block, ToolKindId(1), iron, &registry),
            None,
        );
    }

    #[test]
    fn zero_toughness_instant() {
        let registry = ToolRegistry::new();
        let block = BlockDefinition::new(BlockId(2), "grass").with_toughness(0.0);
        assert_eq!(
            mining_duration(&block, ToolKindId::NONE, ToolTierId::DEFAULT, &registry),
            Some(0.0),
        );
    }
}
