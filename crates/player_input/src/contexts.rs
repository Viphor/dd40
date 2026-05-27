//! Client-only input contexts.
//!
//! These contexts are evaluated by `bevy_enhanced_input` on the client and
//! never replicated. They group the actions that drive purely-local
//! behaviour: the developer free-cam, the pause overlay, mouse look, etc.
//!
//! The networked [`OnFoot`](dd40_input_core::contexts::OnFoot) context
//! lives in `dd40_input_core` because both `dd40_network` and
//! `dd40_player_input` need to refer to the same type. Free-cam and the
//! local UI never cross the wire so they live in the consuming crate.

use bevy::prelude::*;

/// Free-cam input context (developer convenience).
///
/// Active only while [`PlayerMode::FreeCam`](crate::state::PlayerMode) is
/// the current player mode. The character continues to exist but receives
/// no movement / jump / sprint input because [`OnFoot`] is deactivated
/// during free-cam.
///
/// [`OnFoot`]: dd40_input_core::contexts::OnFoot
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
#[reflect(Component)]
pub struct FreeCam;

/// Local-UI input context — always active on the local player.
///
/// Groups actions that drive the game window itself rather than the
/// character: mouse look, pause toggle, mode toggle. These actions never
/// replicate; they only steer client-local state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
#[reflect(Component)]
pub struct LocalUi;
