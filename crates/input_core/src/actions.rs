//! Action vocabulary used throughout dd40.
//!
//! Every action is a `bevy_enhanced_input` [`InputAction`] type. Crates that
//! produce input define the keyboard / mouse / gamepad bindings that fire
//! these actions; crates that consume input read
//! [`ActionState<T>`](bevy_enhanced_input::prelude::ActionState) instead of
//! polling raw [`ButtonInput`](bevy::prelude::ButtonInput).
//!
//! The vocabulary is split into two layers:
//!
//! - **Networked actions** ([`Move`], [`Jump`], [`Sprint`], [`Attack`],
//!   [`Place`], [`Interact`]) — registered with lightyear so client input
//!   replicates to the server. These compose into a per-tick
//!   `CharacterInput` intent on both the server-authoritative entity and
//!   the client predicted entity.
//! - **Client-local actions** ([`Look`], [`Pause`], [`ToggleFreeCam`],
//!   [`FreeCamUp`], [`FreeCamDown`]) — never leave the client; they drive
//!   the camera, pause menu, and developer free-cam.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;

// ----------------------------------------------------------------------------
// Networked actions
// ----------------------------------------------------------------------------

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
// Client-local actions
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
