//! Cross-crate `SystemSet`s for ordering input-pipeline systems.
//!
//! These live in this Tier 0 crate so both `dd40_player_input` (which
//! writes the translator) and `dd40_network` (which writes the wire
//! bridge) can order against the same label without depending on each
//! other.

use bevy::prelude::SystemSet;

/// Marks the systems that translate raw `bevy_enhanced_input` action state
/// into `dd40_character_core::controller::CharacterInput` each
/// `FixedPreUpdate` tick.
///
/// `dd40_player_input`'s `apply_actions_to_character_input` system is the
/// canonical member of this set.
///
/// Downstream systems that need the up-to-date `CharacterInput` for the
/// current tick — most importantly the network-side bridge that copies
/// `CharacterInput` into the wire `ActionState` — should order with
/// `.after(InputTranslationSet)`. Without that ordering the bridge ships
/// stale values from the previous tick.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct InputTranslationSet;
