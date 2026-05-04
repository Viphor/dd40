//! Root plugin for the `dd40_character_gui` crate.
//!
//! [`CharacterGuiPlugin`] is the single entry point for this crate. Add it
//! to a client [`App`] to enable every character-keyed visual provided
//! here.

use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_core::state::{AppState, GameState};

use crate::block_highlight::{BlockHighlightConfig, draw_targeted_block_highlight};

/// Plugin that registers every visual provided by `dd40_character_gui`.
///
/// ## What this plugin sets up
///
/// - [`BlockHighlightConfig`] — render-only colours for the highlight and
///   mining break overlay.
/// - [`draw_targeted_block_highlight`] — wireframe cuboid around the local
///   player's targeted block, gated on
///   [`AppState::Playing`] + [`GameState::Running`].
///
/// Add this plugin only on the **client**.  The headless server has no
/// gizmo runtime and would panic if asked to draw.
#[derive(Default)]
pub struct CharacterGuiPlugin;

impl Plugin for CharacterGuiPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, CharacterCorePlugin);

        app.insert_resource(BlockHighlightConfig::default())
            .register_type::<BlockHighlightConfig>();

        let playing_running = in_state(AppState::Playing).and(in_state(GameState::Running));
        app.add_systems(
            Update,
            draw_targeted_block_highlight.run_if(playing_running),
        );
    }
}
