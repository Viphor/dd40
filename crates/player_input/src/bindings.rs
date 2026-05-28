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
//! The network layer only inserts `Player`. Keeping the binding setup
//! (including inserting the `OnFoot` context itself) here means swapping
//! the input crate touches `dd40_player_input` only.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::{
    Action, ActionOf, Bindings, Cardinal, ContextActivity, Negate, Scale,
};
use dd40_character_core::components::Player;
use dd40_input_core::actions::{
    Attack, FreeCamDown, FreeCamUp, HotbarSelect, Interact, Jump, Look, Move, Pause, Place,
    RmbPress, Sprint, ToggleFreeCam, ToggleInventory,
};
use dd40_input_core::contexts::OnFoot;

use crate::contexts::{FreeCam, LocalUi};

/// Default mouse look sensitivity, in radians per device-unit.
///
/// Matches the legacy [`MouseSensitivity::default`] used before the
/// migration so existing players keep the same feel.
///
/// [`MouseSensitivity::default`]: dd40_character_core::face::MouseSensitivity
pub const DEFAULT_MOUSE_SENSITIVITY: f32 = 0.002;

/// Populates a freshly-spawned local [`Player`] with every input action +
/// binding the client needs.
///
/// Runs only on the client because the dedicated server never inserts
/// [`Player`] (it adds `Character` only — see
/// `dd40_network::with_server_replication`).
pub(crate) fn spawn_local_player_input_tree(
    mut commands: Commands,
    players: Query<Entity, Added<Player>>,
) {
    for player in &players {
        commands.entity(player).insert((
            OnFoot,
            FreeCam,
            ContextActivity::<FreeCam>::INACTIVE,
            LocalUi,
        ));

        spawn_on_foot_actions(&mut commands, player);
        spawn_free_cam_actions(&mut commands, player);
        spawn_local_ui_actions(&mut commands, player);
    }
}

fn spawn_on_foot_actions(commands: &mut Commands, ctx: Entity) {
    commands.spawn((
        Action::<Move>::new(),
        ActionOf::<OnFoot>::new(ctx),
        Bindings::spawn(Cardinal::wasd_keys()),
    ));
    commands.spawn((
        Action::<Jump>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::Space],
    ));
    commands.spawn((
        Action::<Sprint>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![KeyCode::ControlLeft],
    ));
    commands.spawn((
        Action::<Attack>::new(),
        ActionOf::<OnFoot>::new(ctx),
        bevy_enhanced_input::prelude::bindings![MouseButton::Left],
    ));
    // Place and Interact have no direct bindings — the LocalUi `RmbPress`
    // action fires on RMB and an observer dispatches to one of them based
    // on the active item.
    commands.spawn((Action::<Place>::new(), ActionOf::<OnFoot>::new(ctx)));
    commands.spawn((Action::<Interact>::new(), ActionOf::<OnFoot>::new(ctx)));
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
    // moving the mouse right yields +yaw and moving it forward yields
    // -pitch (the consumer expects pitch positive = up).
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

    // Hotbar slot selection: nine digit keys, each scaled to emit its
    // 1-based slot index as the action's f32 value. The selection system
    // reacts on the press edge only.
    commands.spawn((
        Action::<HotbarSelect>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::Press::default(),
        bevy_enhanced_input::prelude::bindings![
            (KeyCode::Digit1, Scale::splat(1.0)),
            (KeyCode::Digit2, Scale::splat(2.0)),
            (KeyCode::Digit3, Scale::splat(3.0)),
            (KeyCode::Digit4, Scale::splat(4.0)),
            (KeyCode::Digit5, Scale::splat(5.0)),
            (KeyCode::Digit6, Scale::splat(6.0)),
            (KeyCode::Digit7, Scale::splat(7.0)),
            (KeyCode::Digit8, Scale::splat(8.0)),
            (KeyCode::Digit9, Scale::splat(9.0)),
        ],
    ));
    commands.spawn((
        Action::<ToggleInventory>::new(),
        ActionOf::<LocalUi>::new(ctx),
        bevy_enhanced_input::prelude::Press::default(),
        bevy_enhanced_input::prelude::bindings![KeyCode::KeyE],
    ));
}
