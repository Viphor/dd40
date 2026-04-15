use bevy::color::palettes::basic::YELLOW;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use dd40_core::character::{
    CharacterBuilder, JumpImpulse, MovementSpeed, Player, SpawnPosition,
    controller::CharacterInput,
};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::debug::DebugInfo;
use dd40_core::prelude::*;

pub mod block_interaction;

pub use block_interaction::{
    BlockFace, BlockInteractionConfig, BlockInteractionPlugin, HeldBlock, TargetedBlock,
};

// ---------------------------------------------------------------------------
// Player mode
// ---------------------------------------------------------------------------

/// Controls how the local player's camera and input are handled.
///
/// Toggle between modes at runtime with the **F1** key.
#[derive(States, Debug, Default, Clone, PartialEq, Eq, Hash, Reflect)]
pub enum PlayerMode {
    /// The camera is attached to the player entity and follows its physics-
    /// driven position.  Keyboard input feeds into [`CharacterController`] so
    /// that movement is subject to gravity, block collisions, and the rest of
    /// the physics pipeline.
    #[default]
    Controller,
    /// The camera detaches from the player entity and flies freely.  Position
    /// is updated directly as a function of time — no physics, no collisions.
    /// The player entity remains where it was and continues to be simulated by
    /// the physics pipeline (it will stand or fall on its own).
    FreeCam,
}

// ---------------------------------------------------------------------------
// Camera components
// ---------------------------------------------------------------------------

/// Mouse sensitivity for looking around.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MouseSensitivity(pub f32);

impl Default for MouseSensitivity {
    fn default() -> Self {
        Self(0.002)
    }
}

/// Pitch and yaw angles for the camera.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct CameraRotation {
    pub pitch: f32,
    pub yaw: f32,
}

impl Default for CameraRotation {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Startup systems
// ---------------------------------------------------------------------------

/// Spawns the player entity when the game enters [`AppState::Playing`].
///
/// If a [`SpawnPosition`] resource is present (set by the network layer when
/// the server sends a `SpawnResponse`), the player is placed at that position.
/// Otherwise it falls back to `(0, 84, 0)`.
fn spawn_player(mut commands: Commands, spawn_position: Option<Res<SpawnPosition>>) {
    let position = spawn_position
        .map(|sp| sp.0)
        .unwrap_or(Vec3::new(0.0, 84.0, 0.0));

    debug!("Spawning player at position {:?}", position);
    commands.spawn((
        Player,
        CharacterBuilder::new("Player")
            .transform(Transform::from_translation(position))
            .build(),
        PhysicsBody,
        CharacterCollider,
        Aabb::player(),
        CharacterController::default(),
        JumpImpulse::default(),
        DebugInfo::new("Player Info")
            .with_color(YELLOW.into())
            .add("position", "Player position")
            .add("velocity", "Player velocity")
            .add("impulse", "Player impulse")
            .add("chunk", "Player chunk"),
    ));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 64.0, 0.0),
        CameraRotation::default(),
        MouseSensitivity::default(),
    ));
}

// ---------------------------------------------------------------------------
// Mode lifecycle
// ---------------------------------------------------------------------------

fn on_pause(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut opts) = cursor_options.single_mut() {
        opts.visible = true;
        opts.grab_mode = CursorGrabMode::None;
    }
}

fn on_resume(mut cursor_options: Query<&mut CursorOptions>) {
    if let Ok(mut opts) = cursor_options.single_mut() {
        opts.visible = false;
        opts.grab_mode = CursorGrabMode::Locked;
    }
}

fn pause_on_escape(
    game_state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Running => next_state.set(GameState::Paused),
            GameState::Paused => next_state.set(GameState::Running),
        }
    }
}

/// Toggles between [`PlayerMode::Controller`] and [`PlayerMode::FreeCam`] when
/// **F1** is pressed.
fn toggle_player_mode(
    mode: Res<State<PlayerMode>>,
    mut next_mode: ResMut<NextState<PlayerMode>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_pressed(KeyCode::F1) {
        match mode.get() {
            PlayerMode::Controller => {
                info!("Switching to FreeCam mode");
                next_mode.set(PlayerMode::FreeCam);
            }
            PlayerMode::FreeCam => {
                info!("Switching to Controller mode");
                next_mode.set(PlayerMode::Controller);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Controller mode — input → CharacterController
// ---------------------------------------------------------------------------

/// Reads keyboard and camera state and writes movement intent into the
/// player entity's [`CharacterInput`].
///
/// Runs only in [`PlayerMode::Controller`].  The physics pipeline picks up the
/// intent each `FixedUpdate` tick via `apply_character_controller`.
///
/// Pitch and yaw from [`CameraRotation`] are forwarded into [`CharacterInput`]
/// so the network crate can replicate head orientation without knowing about
/// the camera.
fn player_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Transform, &CameraRotation), With<Camera3d>>,
    mut player_query: Query<&mut CharacterInput, With<Player>>,
) {
    let Ok(mut char_input) = player_query.single_mut() else {
        return;
    };
    let Ok((camera_transform, camera_rotation)) = camera_query.single() else {
        return;
    };

    // Project camera facing onto the horizontal plane.
    let forward = camera_transform.forward();
    let right = camera_transform.right();
    let forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_h = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += forward_h;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= forward_h;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= right_h;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += right_h;
    }

    char_input.movement = direction.normalize_or_zero();

    // OR-assign so a pending jump set earlier this frame is not overwritten.
    char_input.jump |= keyboard.just_pressed(KeyCode::Space);

    char_input.sprint = keyboard.pressed(KeyCode::ControlLeft);

    // Forward orientation so the network crate can replicate it without a
    // camera dependency.
    char_input.pitch = camera_rotation.pitch;
    char_input.yaw = camera_rotation.yaw;
}

// ---------------------------------------------------------------------------
// FreeCam mode — direct camera Transform mutation
// ---------------------------------------------------------------------------

/// Moves the camera entity directly as a function of time, bypassing physics
/// entirely.
///
/// Runs only in [`PlayerMode::FreeCam`].  The camera follows its own facing
/// direction for WASD, with Space/Shift for vertical movement.
fn free_cam_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera_query: Query<(&mut Transform, &MouseSensitivity), With<Camera3d>>,
) {
    let Ok((mut transform, _)) = camera_query.single_mut() else {
        return;
    };

    let forward = transform.forward();
    let right = transform.right();
    let forward_h = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_h = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += forward_h;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= forward_h;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= right_h;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += right_h;
    }
    if keyboard.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) {
        direction -= Vec3::Y;
    }

    let sprint = if keyboard.pressed(KeyCode::ControlLeft) {
        2.0
    } else {
        1.0
    };

    // Use a fixed free-cam speed rather than MovementSpeed so the camera
    // always feels responsive regardless of character stats.
    const FREE_CAM_SPEED: f32 = 10.0;

    if direction != Vec3::ZERO {
        transform.translation +=
            direction.normalize() * FREE_CAM_SPEED * sprint * time.delta_secs();
    }
}

// ---------------------------------------------------------------------------
// Shared camera systems
// ---------------------------------------------------------------------------

/// Handles mouse movement to rotate the camera.
///
/// Runs in both [`PlayerMode::Controller`] and [`PlayerMode::FreeCam`].
fn mouse_look(
    accumulated_mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_query: Query<
        (&mut Transform, &mut CameraRotation, &MouseSensitivity),
        With<Camera3d>,
    >,
    cursor_options: Query<&CursorOptions>,
) {
    let Ok(cursor_option) = cursor_options.single() else {
        return;
    };
    if cursor_option.grab_mode != CursorGrabMode::Locked {
        return;
    }

    let Ok((mut transform, mut rotation, sensitivity)) = camera_query.single_mut() else {
        return;
    };

    let ev = accumulated_mouse_motion;
    rotation.yaw -= ev.delta.x * sensitivity.0;
    rotation.pitch -= ev.delta.y * sensitivity.0;
    rotation.pitch = rotation.pitch.clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );

    transform.rotation = Quat::from_euler(EulerRot::YXZ, rotation.yaw, rotation.pitch, 0.0);
}

/// Syncs the camera translation to the player entity's position.
///
/// Runs only in [`PlayerMode::Controller`].  In [`PlayerMode::FreeCam`] the
/// camera moves independently so this system is suppressed.
fn sync_camera_to_player(
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };
    camera_transform.translation = player_transform.translation + Vec3::new(0.0, 1.6, 0.0);
}

// ---------------------------------------------------------------------------
// Other systems
// ---------------------------------------------------------------------------

fn update_debug_info(
    player_query: Single<(&Transform, &Velocity, &Impulse, &mut DebugInfo), With<Player>>,
) {
    let (transform, velocity, impulse, mut debug_info) = player_query.into_inner();
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
}

fn load_nearby_chunks(
    mut chunk_cache: ResMut<ChunkCache>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_pos = BlockPos::from(player_transform);
    let player_chunk_pos = player_pos.chunk_pos();

    for dz in -1..=1 {
        for dx in -1..=1 {
            let chunk_pos = ChunkPos {
                x: player_chunk_pos.x + dx,
                z: player_chunk_pos.z + dz,
            };
            if !chunk_cache.contains(&chunk_pos) {
                chunk_cache.request(chunk_pos);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Bevy plugin that **only handles input and camera** — it does not spawn a
/// player entity.
///
/// Use this in networked mode where the network crate spawns the character and
/// adds the [`Player`] marker.  All input systems query [`With<Player>`] and
/// will automatically pick up the network-spawned entity.
///
/// For single-player mode, prefer [`PlayerPlugin`] which bundles this with
/// [`PlayerSpawnPlugin`].
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        let playing_and_running = in_state(AppState::Playing).and(in_state(GameState::Running));

        app.add_plugins(BlockInteractionPlugin::default())
            .init_state::<PlayerMode>()
            .register_type::<PlayerMode>()
            .register_type::<MovementSpeed>()
            .register_type::<MouseSensitivity>()
            .register_type::<CameraRotation>()
            // ── Startup ───────────────────────────────────────────────────
            .add_systems(OnEnter(AppState::Playing), setup_camera)
            // ── Pause / resume cursor management ──────────────────────────
            .add_systems(OnEnter(GameState::Paused), on_pause)
            .add_systems(OnEnter(GameState::Running), on_resume)
            // ── PreUpdate ─────────────────────────────────────────────────
            .add_systems(
                PreUpdate,
                load_nearby_chunks.run_if(playing_and_running.clone()),
            )
            // ── Update — always while playing ─────────────────────────────
            .add_systems(
                Update,
                (mouse_look, toggle_player_mode, update_debug_info)
                    .run_if(playing_and_running.clone()),
            )
            .add_systems(Update, pause_on_escape.run_if(in_state(AppState::Playing)))
            // ── Update — Controller mode only ─────────────────────────────
            .add_systems(
                Update,
                (player_input, sync_camera_to_player).run_if(
                    playing_and_running
                        .clone()
                        .and(in_state(PlayerMode::Controller)),
                ),
            )
            // ── Update — FreeCam mode only ────────────────────────────────
            .add_systems(
                Update,
                free_cam_movement.run_if(playing_and_running.and(in_state(PlayerMode::FreeCam))),
            );
    }
}

/// Bevy plugin that **only spawns the player entity** when entering
/// [`AppState::Playing`].
///
/// In networked mode the character is spawned by the network crate, so this
/// plugin should be omitted.  Use [`PlayerInputPlugin`] for input handling
/// in that case.
pub struct PlayerSpawnPlugin;

impl Plugin for PlayerSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), spawn_player);
    }
}

/// Bevy plugin that registers player types, spawns the player on startup, and
/// handles input in both [`PlayerMode::Controller`] and [`PlayerMode::FreeCam`].
///
/// This is the convenience plugin for **single-player** mode.  It combines
/// [`PlayerSpawnPlugin`] and [`PlayerInputPlugin`].
///
/// In **networked** mode, use [`PlayerInputPlugin`] only — the network crate
/// spawns the character and adds the [`Player`] marker, so [`PlayerSpawnPlugin`]
/// is not needed and would create a duplicate entity.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PlayerSpawnPlugin, PlayerInputPlugin));
    }
}
