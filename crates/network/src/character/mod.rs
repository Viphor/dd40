//! Character replication for `dd40_network`.
//!
//! This module provides a fully self-contained implementation of networked
//! character replication using lightyear.  It lives entirely within the
//! network crate and never modifies anything in `dd40_core` or any other
//! crate.
//!
//! # What it does
//!
//! **Server side** — when a client finishes the lightyear handshake
//! ([`Connected`] fires), the server spawns a character entity that carries
//! both the core physics components and the lightyear replication markers.
//! Each fixed tick the server reads the client's [`ActionState<PlayerInput>`]
//! and translates it into a [`CharacterController`] intent.  After physics
//! resolves, the resulting [`Transform`] is written back to [`PlayerPosition`]
//! and [`PlayerRotation`] so lightyear can replicate them.
//!
//! **Client side** — when lightyear creates a [`Predicted`] entity for the
//! local character, the client attaches [`InputMarker<PlayerInput>`] and
//! begins buffering keyboard input each tick.  The same
//! [`apply_input_to_controller`] function that runs on the server also runs
//! on the client's predicted entity, keeping both sides deterministically in
//! sync for rollback prediction.  A separate pass syncs [`PlayerPosition`] to
//! [`Transform`] for [`Interpolated`] remote-player entities so they render
//! correctly.
//!
//! # Plugin
//!
//! Add [`NetworkCharacterPlugin`] to your app — it will automatically include
//! the server sub-plugin when the `server` feature is active and the client
//! sub-plugin when the `client` feature is active.
//!
//! [`Connected`]: lightyear::prelude::server::Connected
//! [`ActionState<PlayerInput>`]: lightyear::prelude::input::native::ActionState
//! [`CharacterController`]: dd40_core::character::controller::CharacterController
//! [`Predicted`]: lightyear::prelude::client::Predicted
//! [`Interpolated`]: lightyear::prelude::client::Interpolated
//! [`InputMarker<PlayerInput>`]: lightyear::prelude::input::native::InputMarker

use bevy::prelude::*;
use dd40_core::character::controller::CharacterController;
use lightyear::prelude::input::native::ActionState;

use crate::protocol::PlayerInput;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub mod client;

// ============================================================================
// SHARED LOGIC
// ============================================================================

/// Translates one tick's worth of [`PlayerInput`] into [`CharacterController`]
/// intent.
///
/// This function **must be identical** on server and client.  Any divergence
/// will cause constant rollback corrections on the controlling client.
///
/// # Rules
///
/// - No random numbers, timestamps, or external state — only the input.
/// - Keep the body of this function in sync with every caller site.
///
/// [`PlayerInput`]: crate::protocol::PlayerInput
pub(crate) fn apply_input_to_controller(
    action: &ActionState<PlayerInput>,
    controller: &mut CharacterController,
) {
    controller.movement = action.0.movement;
    controller.jump = action.0.jump;
    controller.sprint_multiplier = if action.0.sprint { 2.0 } else { 1.0 };
}

// ============================================================================
// PLUGIN
// ============================================================================

/// Plugin that wires character replication into the app.
///
/// Add this to your app **after** [`ServerNetworkPlugin`] or
/// [`ClientNetworkPlugin`].  It enables the appropriate server or client
/// sub-plugin based on the active Cargo features.
///
/// [`ServerNetworkPlugin`]: crate::server::ServerNetworkPlugin
/// [`ClientNetworkPlugin`]: crate::client::ClientNetworkPlugin
pub struct NetworkCharacterPlugin;

impl Plugin for NetworkCharacterPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "server")]
        app.add_plugins(server::ServerCharacterPlugin);

        #[cfg(feature = "client")]
        app.add_plugins(client::ClientCharacterPlugin);
    }
}
