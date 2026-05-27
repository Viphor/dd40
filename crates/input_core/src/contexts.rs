//! Input contexts that are part of the shared vocabulary.
//!
//! A `bevy_enhanced_input` context groups a set of actions and decides when
//! those actions are evaluated. Only contexts whose actions are replicated
//! across the network need to live in this Tier 0 crate — purely
//! client-local contexts (free-cam, pause menu, debug overlays) belong in
//! the consuming crate.
//!
//! ## Why only [`OnFoot`] lives here
//!
//! `dd40_network` must register the networked context with
//! `lightyear_inputs_bei::InputPlugin::<OnFoot>` so the action state
//! replicates between server and client. `dd40_player_input` then defines
//! the keyboard / mouse bindings for the same context and inserts it on
//! the local player. Both crates need to refer to the **same** Rust type;
//! the only place both are allowed to depend on (per the Tier rules) is a
//! foundation crate — this one.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The "on foot" input context — the set of actions a character is allowed
/// to take while playing normally.
///
/// Attached to a player entity, this acts as a `bevy_enhanced_input`
/// context. The networked actions in [`crate::actions`] ([`Move`],
/// [`Jump`], [`Sprint`], [`Attack`], [`Place`], [`Interact`]) are spawned
/// as `Action<A>` entities related to this context via `ActionOf<OnFoot>`.
///
/// The component is intentionally a unit struct: bindings, action insertion,
/// and lifecycle live in the consuming crates.
///
/// [`Move`]: crate::actions::Move
/// [`Jump`]: crate::actions::Jump
/// [`Sprint`]: crate::actions::Sprint
/// [`Attack`]: crate::actions::Attack
/// [`Place`]: crate::actions::Place
/// [`Interact`]: crate::actions::Interact
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
pub struct OnFoot;
