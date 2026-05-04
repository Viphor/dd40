use bevy::prelude::*;
use dd40_character_core::plugin::CharacterCorePlugin;
use dd40_core::plugin::CorePlugin;
use dd40_physics_core::plugin::PhysicsCorePlugin;

use crate::components::{CameraRotation, MouseSensitivity};
use crate::state::PlayerMode;
use crate::systems::{
    add_debug_info, free_cam_movement, load_nearby_chunks, mouse_look, on_pause, on_resume,
    pause_on_escape, player_input, setup_camera, sync_camera_to_player, toggle_player_mode,
    update_local_player_action,
};
use dd40_character_core::system_sets::CharacterRenderSet;
use dd40_core::prelude::{AppState, GameState};
use dd40_item_core::plugin::ItemCorePlugin;

/// Plugin that handles the locally-controlled player's camera and keyboard/mouse
/// input.
///
/// Wires all first-person camera, cursor, mode-switching, and chunk-loading
/// systems.  It does **not** spawn a player entity — use `PlayerSpawnPlugin`
/// from `dd40_player` for that.
///
/// Auto-adds [`CorePlugin`], [`PhysicsCorePlugin`], and [`CharacterCorePlugin`]
/// via [`ensure_plugins!`] if not already present.
#[derive(Default)]
pub struct PlayerMovementPlugin;

impl Plugin for PlayerMovementPlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(
            app,
            CorePlugin,
            PhysicsCorePlugin,
            CharacterCorePlugin,
            ItemCorePlugin
        );

        let playing_and_running =
            in_state(AppState::Playing).and(in_state(GameState::Running));

        app.init_state::<PlayerMode>()
            .register_type::<PlayerMode>()
            .register_type::<MouseSensitivity>()
            .register_type::<CameraRotation>()
            // ── Startup ───────────────────────────────────────────────
            .add_systems(OnEnter(AppState::Playing), setup_camera)
            // ── Cursor management ─────────────────────────────────────
            .add_systems(OnEnter(GameState::Paused), on_pause)
            .add_systems(OnEnter(GameState::Running), on_resume)
            // ── PreUpdate ─────────────────────────────────────────────
            .add_systems(
                PreUpdate,
                load_nearby_chunks.run_if(playing_and_running.clone()),
            )
            // ── Update — always while playing ─────────────────────────
            .add_systems(Update, add_debug_info)
            .add_systems(
                Update,
                (mouse_look, toggle_player_mode)
                    .run_if(playing_and_running.clone()),
            )
            .add_systems(Update, pause_on_escape.run_if(in_state(AppState::Playing)))
            // ── Update — Controller mode only ─────────────────────────
            .add_systems(
                Update,
                (
                    player_input,
                    update_local_player_action,
                    sync_camera_to_player.in_set(CharacterRenderSet::CameraSync),
                )
                    .run_if(
                        playing_and_running
                            .clone()
                            .and(in_state(PlayerMode::Controller)),
                    ),
            )
            // ── Update — FreeCam mode only ────────────────────────────
            .add_systems(
                Update,
                free_cam_movement
                    .run_if(playing_and_running.and(in_state(PlayerMode::FreeCam))),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_physics_core::prelude::PhysicsConfig;

    #[test]
    fn player_movement_plugin_auto_adds_physics_core() {
        let mut app = App::new();
        app.add_plugins(PlayerMovementPlugin);
        assert!(
            app.world().contains_resource::<PhysicsConfig>(),
            "PhysicsCorePlugin must be auto-added by PlayerMovementPlugin"
        );
    }

    #[test]
    fn player_movement_plugin_registers_player_mode_state() {
        let mut app = App::new();
        app.add_plugins(PlayerMovementPlugin);
        assert!(
            app.world().contains_resource::<State<PlayerMode>>(),
            "PlayerMode state must be registered"
        );
    }
}
