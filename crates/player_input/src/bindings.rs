//! Action entity + binding spawn for the locally-controlled player.
//!
//! When a [`Player`] entity appears on the client,
//! [`spawn_local_player_input_tree`] populates it with:
//!
//! - The [`OnFoot`] context's `Action<T>` entities + keyboard / mouse
//!   bindings. Lightyear auto-replicates each action up to the server.
//! - The [`FreeCam`] context — initially inactive — and its bindings.
//! - The [`LocalUi`] context and its bindings (mouse look, pause toggle,
//!   mode toggle, RMB dispatch).
//!
//! The network layer only inserts `Player`, `OnFoot`, and
//! `InputMarker<OnFoot>`. Keeping the binding setup here means swapping
//! the input crate touches `dd40_player_input` only.

use bevy::prelude::*;
use bevy_enhanced_input::context::ExternallyMocked;
use bevy_enhanced_input::prelude::{
    Action, ActionOf, Bindings, Cardinal, ContextActivity, Negate, Scale,
};
use dd40_character_core::components::Player;
use dd40_input_core::actions::{
    Attack, CameraRotation, FreeCamDown, FreeCamUp, Interact, Jump, Look, Move, Pause, Place,
    RmbPress, Sprint, ToggleFreeCam,
};
use dd40_input_core::contexts::OnFoot;
use dd40_input_core::prespawn::{
    LocalActionPrespawnRequest, LocalClientId, OnFootAction, on_foot_action_prespawn_hash,
};

use crate::contexts::{FreeCam, LocalUi};

/// Default mouse look sensitivity, in radians per device-unit.
///
/// Matches the legacy [`MouseSensitivity::default`] used before the
/// migration so existing players keep the same feel.
///
/// [`MouseSensitivity::default`]: dd40_character_core::face::MouseSensitivity
pub const DEFAULT_MOUSE_SENSITIVITY: f32 = 0.002;

/// Marker inserted on the player entity once
/// [`spawn_local_player_input_tree`] has built its action set, so the
/// system does not double-spawn on subsequent ticks.
#[derive(Component)]
pub(crate) struct LocalInputTreeBuilt;

/// Populates a freshly-spawned local [`Player`] with every input action +
/// binding the client needs.
///
/// The `OnFoot` action entities are spawned as prespawn mirrors of the
/// server-authoritative copies (see
/// `dd40_network::server::character::spawn_on_foot_action_entities_server`).
/// Each carries a [`LocalActionPrespawnRequest`] marker, which a
/// network-side observer translates into a real
/// `PreSpawned::for_receiver(...)` so the two entities are paired by
/// lightyear's prespawn machinery and the per-tick input pipeline can
/// flow client → server.
///
/// We filter on `Without<LocalInputTreeBuilt>` rather than `Added<Player>`
/// so the system keeps re-trying if [`LocalClientId`] hasn't been
/// published yet by the network layer when the player first appears.
///
/// Runs only on the client because the dedicated server never inserts
/// [`Player`] (it adds `Character` only — see
/// `dd40_network::with_server_replication`).
pub(crate) fn spawn_local_player_input_tree(
    mut commands: Commands,
    players: Query<Entity, (With<Player>, Without<LocalInputTreeBuilt>)>,
    local_id: Option<Res<LocalClientId>>,
) {
    if players.is_empty() {
        return;
    }
    let Some(local_id) = local_id else {
        return;
    };
    for player in &players {
        commands.entity(player).insert((
            FreeCam,
            ContextActivity::<FreeCam>::INACTIVE,
            LocalUi,
            LocalInputTreeBuilt,
        ));

        spawn_on_foot_actions(&mut commands, player, local_id.0);
        spawn_free_cam_actions(&mut commands, player);
        spawn_local_ui_actions(&mut commands, player);
    }
}

fn spawn_on_foot_actions(commands: &mut Commands, ctx: Entity, client_id_bits: u64) {
    let prespawn = |action: OnFootAction| {
        LocalActionPrespawnRequest::new(
            on_foot_action_prespawn_hash(client_id_bits, action),
            ctx,
        )
    };

    commands.spawn((
        Action::<Move>::new(),
        ActionOf::<OnFoot>::new(ctx),
        Bindings::spawn(Cardinal::wasd_keys()),
        prespawn(OnFootAction::Move),
    ));
    commands.spawn((
        Action::<Jump>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::Space],
        prespawn(OnFootAction::Jump),
    ));
    commands.spawn((
        Action::<Sprint>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::ControlLeft],
        prespawn(OnFootAction::Sprint),
    ));
    commands.spawn((
        Action::<Attack>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![MouseButton::Left],
        prespawn(OnFootAction::Attack),
    ));
    // Place and Interact have no direct bindings — the LocalUi `RmbPress`
    // action fires on RMB and an observer dispatches to one of them based
    // on the active item.
    commands.spawn((
        Action::<Place>::new(),
        ActionOf::<OnFoot>::new(ctx),
        prespawn(OnFootAction::Place),
    ));
    commands.spawn((
        Action::<Interact>::new(),
        ActionOf::<OnFoot>::new(ctx),
        prespawn(OnFootAction::Interact),
    ));
    // CameraRotation has no direct binding — the client bridges
    // CharacterInput::{yaw,pitch} into it each tick (see
    // `dd40_network::client::character::bridge_camera_rotation_to_action`).
    //
    // `ExternallyMocked` tells BEI's `EnhancedInputSystems::Update` to skip
    // this action; otherwise the absence of bindings would cause Update to
    // zero out `ActionValue` every tick, clobbering the bridge write before
    // `BufferClientInputs` captures it for the input message.
    commands.spawn((
        Action::<CameraRotation>::new(),
        ActionOf::<OnFoot>::new(ctx),
        ExternallyMocked,
        prespawn(OnFootAction::CameraRotation),
    ));
}

fn spawn_free_cam_actions(commands: &mut Commands, ctx: Entity) {
    commands.spawn((
        Action::<Move>::new(),
        ActionOf::<FreeCam>::new(ctx),
        Bindings::spawn(Cardinal::wasd_keys()),
    ));
    commands.spawn((
        Action::<Sprint>::new(),
        ActionOf::<FreeCam>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::ControlLeft],
    ));
    commands.spawn((
        Action::<FreeCamUp>::new(),
        ActionOf::<FreeCam>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::Space],
    ));
    commands.spawn((
        Action::<FreeCamDown>::new(),
        ActionOf::<FreeCam>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::ShiftLeft],
    ));
}

fn spawn_local_ui_actions(commands: &mut Commands, ctx: Entity) {
    // Look: raw mouse motion → Vec2, scaled by sensitivity and negated so
    // moving the mouse right yields +yaw (`CameraRotation.x`) and moving
    // it forward yields -pitch (`CameraRotation.y`).
    commands.spawn((
        Action::<Look>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::bindings![(
            bevy_enhanced_input::prelude::Binding::mouse_motion(),
            Scale::splat(DEFAULT_MOUSE_SENSITIVITY),
            Negate::all(),
        )],
    ));
    commands.spawn((
        Action::<Pause>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::Press::default(),
        bevy_enhanced_input::prelude::bindings![KeyCode::Escape],
    ));
    commands.spawn((
        Action::<ToggleFreeCam>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::Press::default(),
        bevy_enhanced_input::prelude::bindings![KeyCode::F1],
    ));
    commands.spawn((
        Action::<RmbPress>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::Press::default(),
        bevy_enhanced_input::prelude::bindings![MouseButton::Right],
    ));
}
