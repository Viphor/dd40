//! Action vocabulary used throughout dd40.
//!
//! Every action is a `bevy_enhanced_input` [`InputAction`] type. Crates that
//! produce input define the keyboard / mouse / gamepad bindings that fire
//! these actions; crates that consume input read
//! [`ActionState<T>`](bevy_enhanced_input::prelude::ActionState) instead of
//! polling raw [`ButtonInput`](bevy::prelude::ButtonInput).
//!
//! All actions in this module are **client-side only**: BEI evaluates them
//! on the player's machine, `dd40_player_input` folds them into the
//! per-tick `CharacterInput` intent, and `dd40_network` ships that intent
//! to the server as an `ActionState<PlayerInput>` (lightyear
//! `input_native`). The server does **not** evaluate BEI; it consumes the
//! wire `PlayerInput` directly. Action types live here as the single
//! source of truth for the client's input vocabulary so that bindings,
//! the translator, and any future input source (gamepad, scripting, demo
//! replay) all refer to the same symbols.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;

/// Planar movement intent in **local character space**.
///
/// `x` is strafe (right positive), `y` is forward (forward positive). The
/// magnitude is unconstrained in the action; callers are expected to
/// normalise as appropriate.
#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct Move;

/// Begin a jump on the next physics tick.
///
/// Fired as a one-shot when the binding transitions to pressed. Holding the
/// key does not auto-repeat at the action layer — repeat semantics, if any,
/// belong to the translator.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Jump;

/// Sprint while held.
///
/// Continuous: stays active for as long as the binding is held.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Sprint;

/// Primary action — mining / attacking the targeted block or entity.
///
/// Continuous: held for the duration of a mining gesture.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Attack;

/// Place the active item's block in the world.
///
/// Fired as a one-shot. The client decides whether RMB triggers [`Place`]
/// or [`Interact`] based on the active item — see `dd40_player_input` for
/// that dispatch.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Place;

/// Interact with the targeted block or entity (open container, toggle
/// lever, …).
///
/// Fired as a one-shot. See [`Place`] for the RMB dispatch policy.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Interact;

// ----------------------------------------------------------------------------
// UI / camera actions
// ----------------------------------------------------------------------------

/// Camera look delta in **device units per frame**.
///
/// `x` is yaw, `y` is pitch. Sensitivity is applied at the binding layer as
/// a BEI modifier, not by the consumer.
#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct Look;

/// Toggle the pause overlay / release the cursor.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Pause;

/// Toggle between controller and free-cam player modes (developer
/// convenience).
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct ToggleFreeCam;

/// Move the free-cam upward (world `+y`) while held.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct FreeCamUp;

/// Move the free-cam downward (world `-y`) while held.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct FreeCamDown;

/// Internal client-only action that fires on a right-mouse-button press.
///
/// Observed by `dd40_player_input` to dispatch to either
/// [`Place`](crate::actions::Place) or
/// [`Interact`](crate::actions::Interact) depending on whether the
/// active item is placeable. Never bound directly to either action so the
/// dispatch policy stays in one place.
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct RmbPress;
