//! Root plugin for the `dd40_character_gui` crate.
//!
//! [`CharacterGuiPlugin`] is the single entry point for this crate. Add it
//! to a client [`App`] to enable every character-keyed visual provided
//! here.

use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

/// Plugin that registers all systems and resources provided by
/// `dd40_character_gui`.
///
/// ## What this plugin sets up
///
/// - TODO: targeted-block highlight gizmo (added in the next slice)
/// - TODO: mining break overlay gizmo (added in a later slice)
#[derive(Default)]
pub struct CharacterGuiPlugin;

impl Plugin for CharacterGuiPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, CharacterCorePlugin);
    }
}
