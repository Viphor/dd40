use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::block::events::{
    AbortMiningRequest, BlockPlaced, BlockRemoved, MineBlockRequest, PlaceBlockRequest,
    StartMiningRequest,
};
use dd40_core::plugin::CorePlugin;
use dd40_core::prelude::*;

use crate::mining::{apply_removed_blocks, update_mining};
pub use dd40_character_core::mining_state::MiningState;
pub use dd40_character_core::targeted_block::{BlockFace, TargetedBlock};
use crate::placement::{HeldBlock, apply_placed_blocks, try_place_block};
use crate::targeting::{
    BlockInteractionConfig, draw_targeted_block_highlight, spawn_debug_entity,
    update_debug_info, update_targeted_block,
};

/// Plugin that adds block-targeting, highlight rendering, placement, and
/// mining for any entity with a [`Character`] marker.
///
/// Unlike the old `BlockInteractionPlugin`, this plugin does **not** gate
/// systems on `PlayerMode`. That concern belongs to the caller — wire the
/// systems under whatever condition suits your control scheme (e.g. only while
/// `PlayerMode::Controller` for human players).
///
/// Registers the following resources:
/// - [`BlockInteractionConfig`] — raycast reach and highlight colour.
/// - [`TargetedBlock`]          — the block and face the character is looking at.
/// - [`HeldBlock`]              — the block type to place on right-click.
/// - [`MiningState`]            — current mining progress (readable by HUD / renderer).
///
/// Registers the following messages:
/// - [`PlaceBlockRequest`]     — written here, consumed by the network layer.
/// - [`BlockPlaced`]           — written by the network layer; consumed here.
/// - [`BlockRemoved`]          — written by the network layer; consumed here.
/// - [`StartMiningRequest`]    — written here, consumed by the network layer.
/// - [`AbortMiningRequest`]    — written here, consumed by the network layer.
/// - [`MineBlockRequest`]      — written here, consumed by the network layer.
///
/// All gameplay systems run only while [`AppState::Playing`] **and**
/// [`GameState::Running`]. [`apply_removed_blocks`] runs whenever
/// [`AppState::Playing`] so that block removals from other players are applied
/// even while the local game is paused.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_character_interaction::CharacterInteractionPlugin;
///
/// App::new()
///     .add_plugins(CharacterInteractionPlugin::default())
///     .run();
/// ```
///
/// [`Character`]: dd40_character_core::components::Character
/// [`ChunkCache`]: dd40_core::chunk::cache::ChunkCache
#[derive(Default)]
pub struct CharacterInteractionPlugin;

impl Plugin for CharacterInteractionPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin, CharacterCorePlugin);

        // ── Resources ─────────────────────────────────────────────────────
        // MiningState and TargetedBlock are per-character Components, attached
        // via CharacterBundle in dd40_character_core; do not insert as resources.
        app.insert_resource(BlockInteractionConfig::default())
            .insert_resource(HeldBlock::default())
            .register_type::<BlockInteractionConfig>()
            .register_type::<HeldBlock>();

        // ── Messages ──────────────────────────────────────────────────────
        app.add_message::<PlaceBlockRequest>();
        app.add_message::<BlockPlaced>();
        app.add_message::<BlockRemoved>();
        app.add_message::<StartMiningRequest>();
        app.add_message::<AbortMiningRequest>();
        app.add_message::<MineBlockRequest>();

        // ── Startup ───────────────────────────────────────────────────────
        app.add_systems(Startup, spawn_debug_entity);

        // ── Per-frame gameplay systems ────────────────────────────────────
        let playing_running = in_state(AppState::Playing).and(in_state(GameState::Running));

        app.add_systems(
            Update,
            (
                update_targeted_block,
                draw_targeted_block_highlight,
                update_debug_info,
                try_place_block,
                update_mining,
            )
                .chain()
                .run_if(playing_running.clone()),
        );

        app.add_systems(PostUpdate, apply_placed_blocks.run_if(playing_running));

        let playing = in_state(AppState::Playing);
        app.add_systems(PostUpdate, apply_removed_blocks.run_if(playing));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::plugin::CorePlugin;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app
    }

    #[test]
    fn character_interaction_plugin_auto_adds_core() {
        let mut app = make_app();
        app.add_plugins(CharacterInteractionPlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
        assert!(app.is_plugin_added::<CharacterCorePlugin>());
    }

    #[test]
    fn character_interaction_plugin_inserts_resources() {
        let mut app = make_app();
        app.add_plugins(CharacterInteractionPlugin);
        app.update();
        assert!(app.world().contains_resource::<BlockInteractionConfig>());
        assert!(app.world().contains_resource::<HeldBlock>());
    }

    #[test]
    fn character_interaction_plugin_does_not_double_add_core_when_already_present() {
        let mut app = make_app();
        app.add_plugins(CorePlugin);
        app.add_plugins(CharacterCorePlugin);
        // Adding the plugin when its deps are already registered must not panic.
        app.add_plugins(CharacterInteractionPlugin);
        app.update();
    }
}
