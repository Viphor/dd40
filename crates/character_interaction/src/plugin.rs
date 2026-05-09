use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::block::events::{
    AbortMiningRequest, BlockPlaced, BlockRemoved, MineBlockRequest, StartMiningRequest,
};
use dd40_core::plugin::CorePlugin;
use dd40_core::prelude::*;
use dd40_item_core::plugin::ItemCorePlugin;

use crate::interact::try_interact;
use crate::mining::{apply_removed_blocks, update_mining};
pub use dd40_character_core::mining_state::MiningState;
pub use dd40_character_core::targeted_block::{BlockFace, TargetedBlock};
use crate::placement::{apply_placed_blocks, try_place_block};
use crate::targeting::{
    BlockInteractionConfig, spawn_debug_entity, update_debug_info, update_targeted_block,
};
use crate::validators::character_collision_validator;
use dd40_core::chunk::ChunkAuthorityAppExt;

/// Plugin that adds block-targeting, highlight rendering, placement, and
/// mining for any entity with a [`Character`] marker.
///
/// Unlike the old `BlockInteractionPlugin`, this plugin does **not** gate
/// systems on `PlayerMode`. That concern belongs to the caller — wire the
/// systems under whatever condition suits your control scheme (e.g. only while
/// `PlayerMode::Controller` for human players).
///
/// Registers the following resources:
/// - [`BlockInteractionConfig`] — raycast reach (gameplay-only). The
///   targeted-block highlight gizmo lives in
///   [`dd40_character_gui::plugin::CharacterGuiPlugin`][^gui] and owns its
///   own render-only config.
///
/// [^gui]: crate documented in the `dd40_character_gui` crate.
///
/// Per-character components (attached via [`CharacterBundle`]):
/// - [`TargetedBlock`] — the block and face the character is looking at.
/// - [`MiningState`]   — current mining progress (readable by HUD / renderer).
///
/// Mining and placement read each character's
/// [`ActiveItem`][dd40_item_core::active_item::ActiveItem] to determine the
/// effective tool kind/tier and the placeable block. A character with no
/// [`ActiveItem`] is treated as bare hands holding nothing.
///
/// Registers the following messages:
/// - [`BlockPlaced`]           — written by the network layer; consumed here.
/// - [`BlockRemoved`]          — written by the network layer; consumed here.
/// - [`StartMiningRequest`]    — written here, consumed by the network layer.
/// - [`AbortMiningRequest`]    — written here, consumed by the network layer.
/// - [`MineBlockRequest`]      — written here, consumed by the network layer.
///
/// Block **placement** does not go through a request message: the
/// `try_place_block` system pushes a predicted [`ChunkChange`] onto the local
/// [`ChunkCache`] directly. The server runs the same system against the
/// replicated [`CharacterInput`][dd40_character_core::components::CharacterInput]
/// and commits authoritatively via the chunk-authority pipeline.
///
/// [`ChunkChange`]: dd40_core::chunk::ChunkChange
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
/// [`CharacterBundle`]: dd40_character_core::bundles::CharacterBundle
/// [`ChunkCache`]: dd40_core::chunk::cache::ChunkCache
#[derive(Default)]
pub struct CharacterInteractionPlugin;

impl Plugin for CharacterInteractionPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin, CharacterCorePlugin, ItemCorePlugin);

        // ── Resources ─────────────────────────────────────────────────────
        // MiningState and TargetedBlock are per-character Components, attached
        // via CharacterBundle in dd40_character_core; do not insert as resources.
        app.insert_resource(BlockInteractionConfig::default())
            .register_type::<BlockInteractionConfig>();

        // ── Messages ──────────────────────────────────────────────────────
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
                update_debug_info,
                try_place_block,
                try_interact,
                update_mining,
            )
                .chain()
                .run_if(playing_running.clone()),
        );

        app.add_systems(PostUpdate, apply_placed_blocks.run_if(playing_running));

        let playing = in_state(AppState::Playing);
        app.add_systems(PostUpdate, apply_removed_blocks.run_if(playing));

        // Gated on PendingChunkRejections so the registration is harmless
        // on instances without ChunkAuthorityPlugin (e.g. clients).
        app.add_chunk_change_validator_system(
            character_collision_validator
                .run_if(resource_exists::<dd40_core::chunk::PendingChunkRejections>),
        );
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
        assert!(app.is_plugin_added::<ItemCorePlugin>());
    }

    #[test]
    fn character_interaction_plugin_inserts_resources() {
        let mut app = make_app();
        app.add_plugins(CharacterInteractionPlugin);
        app.update();
        assert!(app.world().contains_resource::<BlockInteractionConfig>());
    }

    #[test]
    fn character_interaction_plugin_does_not_double_add_core_when_already_present() {
        let mut app = make_app();
        app.add_plugins(CorePlugin);
        app.add_plugins(CharacterCorePlugin);
        app.add_plugins(ItemCorePlugin);
        // Adding the plugin when its deps are already registered must not panic.
        app.add_plugins(CharacterInteractionPlugin);
        app.update();
    }

    /// Regression test for the headless-server gizmo panic.
    ///
    /// `MinimalPlugins` provides no `bevy_gizmos` runtime, so any system in
    /// `CharacterInteractionPlugin` that asks for `Res<GizmoConfigStore>`
    /// would panic when the gameplay schedule runs.  Forcing the state into
    /// `Playing` + `Running` exercises every gameplay system.
    #[test]
    fn character_interaction_plugin_runs_under_minimal_plugins_in_playing_state() {
        use dd40_core::state::{AppState, GameState};

        let mut app = make_app();
        app.add_plugins(CharacterInteractionPlugin);

        // Drive the state machine into Playing/Running.
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Running);
        app.update(); // applies state transitions
        app.update(); // first tick where gameplay systems are eligible to run
    }
}
