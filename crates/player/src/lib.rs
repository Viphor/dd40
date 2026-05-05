use bevy::prelude::*;
use dd40_character_core::{
    builder::CharacterBuilder,
    components::{Player, SpawnPosition},
};
use dd40_character_interaction::{CharacterInteractionPlugin, TargetedBlock};
use dd40_character_core::mining_state::MiningState;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;
use dd40_physics_core::character_ext::CharacterPhysicsExt;
use dd40_physics_core::prelude::{Impulse, Velocity};
use dd40_player_movement::{PlayerMode, PlayerMovementPlugin};

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use dd40_character_interaction::{
    BlockFace, BlockInteractionConfig, CharacterInteractionPlugin as BlockInteractionPlugin,
    TargetedBlock as TargetBlock,
};
pub use dd40_character_core::face::{CameraRotation, MouseSensitivity};
pub use dd40_player_movement::PlayerMode as PlayerModeType;

// ── Startup ───────────────────────────────────────────────────────────────────

/// Spawns the player entity when entering [`AppState::Playing`].
///
/// Uses [`SpawnPosition`] if set by the network layer, otherwise falls back to
/// `(0, 84, 0)`.
fn spawn_player(mut commands: Commands, spawn_position: Option<Res<SpawnPosition>>) {
    let position = spawn_position
        .map(|sp| sp.0)
        .unwrap_or(Vec3::new(0.0, 84.0, 0.0));

    debug!("Spawning player at position {:?}", position);
    CharacterBuilder::new("Player")
        .transform(Transform::from_translation(position))
        .with_physics()
        .with_controller()
        .with_player()
        .with_extra(|entity| {
            entity.insert(
                DebugInfo::new("Player Info")
                    .with_color(bevy::color::palettes::basic::YELLOW.into())
                    .add("position", "Player position")
                    .add("velocity", "Player velocity")
                    .add("impulse", "Player impulse")
                    .add("chunk", "Player chunk")
                    .add("mining", "Idle"),
            );
        })
        .spawn(&mut commands);
}

// ── Per-frame systems ─────────────────────────────────────────────────────────

/// Updates the debug info overlay each frame for the local player.
///
/// Reads physics state and mining progress — this is the only place that needs
/// both `dd40_physics_core` types and `dd40_character_interaction` types, so it
/// lives in this wrapper crate.
fn update_debug_info(
    player_query: Single<(&Transform, &Velocity, &Impulse, &MiningState, &mut DebugInfo), With<Player>>,
) {
    let (transform, velocity, impulse, mining, mut debug_info) = player_query.into_inner();
    let pos = transform.translation;
    debug_info.set(
        "position",
        format!("({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z),
    );
    debug_info.set(
        "velocity",
        format!("({:.1}, {:.1}, {:.1})", velocity.x, velocity.y, velocity.z),
    );
    debug_info.set(
        "impulse",
        format!("({:.1}, {:.1}, {:.1})", impulse.x, impulse.y, impulse.z),
    );
    let chunk = BlockPos::from(transform).chunk_pos();
    debug_info.set("chunk", chunk.to_string());

    let mining_text = match mining {
        MiningState::Idle => "Idle".to_string(),
        MiningState::Mining { pos, progress, .. } => {
            format!("{} ({:.0}%)", pos, progress * 100.0)
        }
    };
    debug_info.set("mining", mining_text);
}

// ── Plugins ───────────────────────────────────────────────────────────────────

/// Bevy plugin that handles input and camera — does **not** spawn a player
/// entity.
///
/// Use this in networked mode where the network crate spawns the character and
/// adds the [`Player`] marker. All input systems query `With<Player>` and
/// automatically pick up the network-spawned entity.
///
/// For single-player mode, prefer [`PlayerPlugin`] which bundles this with
/// [`PlayerSpawnPlugin`].
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        let playing_and_running = in_state(AppState::Playing).and(in_state(GameState::Running));

        app.add_plugins((PlayerMovementPlugin, CharacterInteractionPlugin));

        // Clear interaction state when switching to FreeCam so highlights and
        // mining progress don't linger. OnEnter lives here because it needs
        // both PlayerMode (player_movement) and the interaction resources.
        app.add_systems(
            OnEnter(PlayerMode::FreeCam),
            |mut player_query: Query<(&mut TargetedBlock, &mut MiningState), With<Player>>| {
                if let Ok((mut targeted, mut mining)) = player_query.single_mut() {
                    *targeted = TargetedBlock::default();
                    *mining = MiningState::Idle;
                }
            },
        );

        app.add_systems(
            Update,
            update_debug_info.run_if(playing_and_running),
        );
    }
}

/// Bevy plugin that **only spawns the player entity** when entering
/// [`AppState::Playing`].
///
/// In networked mode the character is spawned by the network crate, so this
/// plugin should be omitted. Use [`PlayerInputPlugin`] for input handling in
/// that case.
pub struct PlayerSpawnPlugin;

impl Plugin for PlayerSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_player);
    }
}

/// Convenience plugin for **single-player** mode. Combines [`PlayerSpawnPlugin`]
/// and [`PlayerInputPlugin`].
///
/// In **networked** mode, use [`PlayerInputPlugin`] only — the network crate
/// spawns the character and adds the [`Player`] marker, so [`PlayerSpawnPlugin`]
/// would create a duplicate entity.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PlayerSpawnPlugin, PlayerInputPlugin));
    }
}
