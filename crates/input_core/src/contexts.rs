//! Input contexts that are part of the shared vocabulary.
//!
//! A `bevy_enhanced_input` context groups a set of actions and decides when
//! those actions are evaluated. Only [`OnFoot`] lives in this Tier 0 crate
//! today; purely client-local contexts (free-cam, pause menu, debug
//! overlays) belong in the consuming crate.
//!
//! ## Why [`OnFoot`] lives here
//!
//! `OnFoot` is the input context that defines "the actions a character is
//! allowed to take while playing normally". Today only `dd40_player_input`
//! (the client-side binding owner) and `dd40_player_input`'s translator
//! refer to it, but any future client-side input source (gamepad, scripted
//! input, demo replay) will want the same Rust type so it can be swapped in
//! without changes elsewhere. Living in this foundation crate keeps the
//! symbol available without forcing a Tier 1 → Tier 1 dependency.
//!
//! `OnFoot` is **not** transmitted over the network — the wire format is
//! `ActionState<PlayerInput>` (see `dd40_network`).

use bevy::prelude::*;

/// The "on foot" input context — the set of actions a character is allowed
/// to take while playing normally.
///
/// Attached to the local player entity, this acts as a
/// `bevy_enhanced_input` context. The character actions in
/// [`crate::actions`] ([`Move`], [`Jump`], [`Sprint`], [`Attack`],
/// [`Place`], [`Interact`]) are spawned as `Action<A>` entities related to
/// this context via `ActionOf<OnFoot>`.
///
/// The component is intentionally a unit struct: bindings, action
/// insertion, and lifecycle live in the consuming crates.
///
/// [`Move`]: crate::actions::Move
/// [`Jump`]: crate::actions::Jump
/// [`Sprint`]: crate::actions::Sprint
/// [`Attack`]: crate::actions::Attack
/// [`Place`]: crate::actions::Place
/// [`Interact`]: crate::actions::Interact
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
#[reflect(Component)]
pub struct OnFoot;
