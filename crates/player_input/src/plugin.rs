use bevy::prelude::*;
use bevy_enhanced_input::EnhancedInputSystems;
use bevy_enhanced_input::context::InputContextAppExt;
use dd40_character_core::components::Player;
use dd40_character_core::mining_state::MiningState;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::plugin::CorePlugin;
use dd40_input_core::contexts::OnFoot;
use dd40_input_core::plugin::InputCorePlugin;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::bindings::spawn_local_player_input_tree;
use crate::contexts::{FreeCam, LocalUi};
use crate::state::PlayerMode;
use crate::systems::{
    add_debug_info, free_cam_movement, load_nearby_chunks, mouse_look, on_pause, on_pause_action,
    on_resume, on_rmb_press, on_toggle_free_cam_action, setup_camera, sync_camera_to_face,
    sync_context_activity_to_mode,
};
use crate::translation::apply_actions_to_character_input;
use dd40_character_core::system_sets::CharacterRenderSet;
use dd40_core::prelude::{AppState, GameState};
use dd40_item_core::plugin::ItemCorePlugin;

/// Translation-only plugin: BEI action state → [`CharacterInput`] every
/// [`FixedPreUpdate`] tick.
///
/// This plugin is **headless-safe**: it pulls in no windowing, no camera,
/// no [`ButtonInput`] reads. Both client and server binaries add it so the
/// same translation runs on the server-authoritative character entity and
/// on the client predicted entity — a hard requirement for prediction /
/// rollback convergence.
///
/// ## What this plugin sets up
///
/// - Auto-adds [`CorePlugin`], [`InputCorePlugin`], and
///   [`CharacterCorePlugin`] via [`ensure_plugins!`].
/// - Registers [`OnFoot`] as a `bevy_enhanced_input` context evaluated in
///   [`FixedPreUpdate`] (idempotent — guarded by a marker resource so
///   adding the plugin twice does not panic).
/// - Adds [`apply_actions_to_character_input`] in [`FixedPreUpdate`] after
///   [`EnhancedInputSystems::Apply`].
///
/// [`ensure_plugins!`]: dd40_core::ensure_plugins
/// [`CharacterInput`]: dd40_character_core::controller::CharacterInput
#[derive(Default)]
pub struct PlayerInputTranslationPlugin;

/// Marker resource ensuring the [`OnFoot`] context is only registered once
/// even if [`PlayerInputTranslationPlugin`] is added by multiple plugins.
#[derive(Resource)]
struct OnFootContextRegistered;

impl Plugin for PlayerInputTranslationPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin, InputCorePlugin, CharacterCorePlugin);

        if !app.world().contains_resource::<OnFootContextRegistered>() {
            app.add_input_context_to::<FixedPreUpdate, OnFoot>();
            app.insert_resource(OnFootContextRegistered);
        }

        app.register_type::<OnFoot>().add_systems(
            FixedPreUpdate,
            apply_actions_to_character_input
                .in_set(dd40_input_core::system_sets::InputTranslationSet)
                .after(EnhancedInputSystems::Apply),
        );
    }
}

/// Plugin that handles the locally-controlled player's camera and keyboard/mouse
/// input.
///
/// Wires all first-person camera, cursor, mode-switching, and chunk-loading
/// systems.  It does **not** spawn a player entity — the network layer
/// is responsible for spawning the character.
///
/// Auto-adds [`CorePlugin`], [`PhysicsCorePlugin`], [`CharacterCorePlugin`],
/// [`InputCorePlugin`], [`PlayerInputTranslationPlugin`], and
/// [`ItemCorePlugin`] via [`ensure_plugins!`] if not already present.
///
/// [`ensure_plugins!`]: dd40_core::ensure_plugins
#[derive(Default)]
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(
            app,
            CorePlugin,
            PhysicsCorePlugin,
            CharacterCorePlugin,
            InputCorePlugin,
            PlayerInputTranslationPlugin,
            ItemCorePlugin
        );

        // Client-local contexts. FreeCam shares OnFoot's FixedPreUpdate
        // schedule because it drives character-style movement; LocalUi
        // runs in PreUpdate (the BEI default) since it gates pause /
        // mode-toggle / mouse-look.
        if !app.world().contains_resource::<FreeCamContextRegistered>() {
            app.add_input_context_to::<FixedPreUpdate, FreeCam>();
            app.insert_resource(FreeCamContextRegistered);
        }
        if !app.world().contains_resource::<LocalUiContextRegistered>() {
            app.add_input_context::<LocalUi>();
            app.insert_resource(LocalUiContextRegistered);
        }

        let playing_and_running = in_state(AppState::Playing).and(in_state(GameState::Running));

        app.init_state::<PlayerMode>()
            .register_type::<PlayerMode>()
            .register_type::<FreeCam>()
            .register_type::<LocalUi>()
            .add_observer(on_pause_action)
            .add_observer(on_toggle_free_cam_action)
            .add_observer(on_rmb_press)
            // ── Startup ───────────────────────────────────────────────
            .add_systems(OnEnter(AppState::Playing), setup_camera)
            // ── Cursor management ─────────────────────────────────────
            .add_systems(OnEnter(GameState::Paused), on_pause)
            .add_systems(OnEnter(GameState::Running), on_resume)
            // ── Spawn input tree for new local players ────────────────
            .add_systems(PreUpdate, spawn_local_player_input_tree)
            // ── PreUpdate ─────────────────────────────────────────────
            .add_systems(
                PreUpdate,
                load_nearby_chunks.run_if(playing_and_running.clone()),
            )
            // ── Update — always while playing ─────────────────────────
            .add_systems(Update, add_debug_info)
            .add_systems(Update, mouse_look.run_if(playing_and_running.clone()))
            // ── Mode transitions ──────────────────────────────────────
            .add_systems(OnEnter(PlayerMode::FreeCam), clear_interaction_state)
            .add_systems(OnEnter(PlayerMode::FreeCam), sync_context_activity_to_mode)
            .add_systems(
                OnEnter(PlayerMode::Controller),
                sync_context_activity_to_mode,
            )
            // ── Update — Controller mode only ─────────────────────────
            .add_systems(
                Update,
                sync_camera_to_face
                    .in_set(CharacterRenderSet::CameraSync)
                    .run_if(
                        playing_and_running
                            .clone()
                            .and(in_state(PlayerMode::Controller)),
                    ),
            )
            // ── Update — FreeCam mode only ────────────────────────────
            .add_systems(
                Update,
                free_cam_movement.run_if(playing_and_running.and(in_state(PlayerMode::FreeCam))),
            );
    }
}

#[derive(Resource)]
struct FreeCamContextRegistered;

#[derive(Resource)]
struct LocalUiContextRegistered;

/// Resets per-frame interaction state on the local player when entering
/// [`PlayerMode::FreeCam`] so stale block highlights and mining progress
/// don't linger after switching out of controller mode.
fn clear_interaction_state(
    mut player_query: Query<(&mut TargetedBlock, &mut MiningState), With<Player>>,
) {
    if let Ok((mut targeted, mut mining)) = player_query.single_mut() {
        *targeted = TargetedBlock::default();
        *mining = MiningState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_physics_core::prelude::PhysicsConfig;

    #[test]
    fn player_input_plugin_auto_adds_physics_core() {
        let mut app = App::new();
        app.add_plugins(PlayerInputPlugin);
        assert!(
            app.world().contains_resource::<PhysicsConfig>(),
            "PhysicsCorePlugin must be auto-added by PlayerInputPlugin"
        );
    }

    #[test]
    fn player_input_plugin_registers_player_mode_state() {
        let mut app = App::new();
        app.add_plugins(PlayerInputPlugin);
        assert!(
            app.world().contains_resource::<State<PlayerMode>>(),
            "PlayerMode state must be registered"
        );
    }
}
