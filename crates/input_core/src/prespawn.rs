//! Deterministic prespawn-hash helpers for networked BEI action entities.
//!
//! lightyear's `bevy_enhanced_input` integration replicates per-player
//! action entities **server → client**, pairing the locally spawned client
//! mirror with the replicated server entity through lightyear's
//! [`PreSpawned`] machinery. The pairing key is a 64-bit hash that **both
//! sides** must compute identically — if they disagree, the client ends
//! up with two action entities (one local, one replicated) and inputs
//! never reach the server.
//!
//! This module owns the hash function so the server (in `dd40_network`)
//! and the client (in `dd40_player_input`) cannot drift out of sync.
//!
//! It also exposes [`LocalActionPrespawnRequest`] — a Bevy component the
//! client-side input crate attaches to a freshly spawned local
//! [`Action`] entity to ask the network layer to install the appropriate
//! `PreSpawned::for_receiver(...)` metadata. Using a marker component
//! lets `dd40_player_input` request prespawn-matching without taking a
//! direct dependency on lightyear.
//!
//! ## Stability
//!
//! The hash function is part of the network protocol's identity. Changing
//! the salt, the mix constant, or any of the [`OnFootAction`] indices
//! breaks compatibility with any peer that still computes the old hash.
//! Treat changes here the same as a protocol-version bump.
//!
//! [`Action`]: https://docs.rs/bevy_enhanced_input
//! [`PreSpawned`]: https://docs.rs/lightyear

use bevy::prelude::{Component, Entity, Resource};

/// Resource the network layer publishes after the local client has
/// completed the lightyear handshake, exposing its `PeerId` as a raw
/// `u64` to crates that must not depend on lightyear directly.
///
/// `dd40_player_input` reads this to compute the prespawn hashes that
/// must match the server's hashes — see [`on_foot_action_prespawn_hash`].
///
/// The resource is inserted by `dd40_network::client` and removed on
/// disconnect.
#[derive(Resource, Debug, Clone, Copy)]
pub struct LocalClientId(pub u64);

/// Identifies a single networked action in the [`OnFoot`] context for
/// prespawn-hash purposes.
///
/// The discriminant doubles as the per-action salt fed into
/// [`on_foot_action_prespawn_hash`]. The discriminants must remain stable
/// for as long as the surrounding protocol version does — see the module
/// docs.
///
/// [`OnFoot`]: crate::contexts::OnFoot
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OnFootAction {
    /// [`crate::actions::Move`]
    Move = 1,
    /// [`crate::actions::Jump`]
    Jump = 2,
    /// [`crate::actions::Sprint`]
    Sprint = 3,
    /// [`crate::actions::Attack`]
    Attack = 4,
    /// [`crate::actions::Place`]
    Place = 5,
    /// [`crate::actions::Interact`]
    Interact = 6,
    /// [`crate::actions::CameraRotation`]
    CameraRotation = 7,
}

/// Domain salt mixed into every [`on_foot_action_prespawn_hash`] result so
/// these hashes never collide with prespawn hashes used by other game
/// systems (loose items, projectiles, …).
///
/// Bytes spell `"dd40-bei"` in big-endian ASCII.
const HASH_SALT: u64 = 0x6464_3430_2D62_6569;

/// Large odd multiplier mixed with the client id to spread bits before
/// folding in the per-action salt. Same constant used by the reference
/// lightyear BEI example.
const HASH_MIX: u64 = 6_364_136_223_846_793_005;

/// Returns the stable prespawn-hash for the given action of the
/// [`OnFoot`] context, owned by the client whose lightyear `PeerId` has
/// the bit pattern `client_id_bits`.
///
/// The result is purely a function of its inputs — no time, randomness,
/// or external state — so the client and the server always agree.
///
/// `client_id_bits` is a `u64` rather than a typed `PeerId` so this
/// crate need not depend on lightyear.
///
/// [`OnFoot`]: crate::contexts::OnFoot
#[inline]
pub fn on_foot_action_prespawn_hash(client_id_bits: u64, action: OnFootAction) -> u64 {
    client_id_bits
        .wrapping_mul(HASH_MIX)
        .wrapping_add(HASH_SALT)
        .wrapping_add(action as u64)
}

/// Component used by the client-side input crate to ask the network
/// layer to attach lightyear's `PreSpawned::for_receiver(owner)` metadata
/// to the entity it is added to.
///
/// `dd40_player_input` spawns its local mirror of each networked action
/// with this marker; an observer in `dd40_network::client::character`
/// translates it into a real `PreSpawned` component (and removes the
/// marker afterwards). Keeping the marker in this crate lets the input
/// layer stay lightyear-free.
///
/// Construct via [`Self::new`].
#[derive(Component, Debug, Clone, Copy)]
pub struct LocalActionPrespawnRequest {
    /// Stable prespawn hash — must equal the hash computed on the server
    /// for the matching action entity. Use
    /// [`on_foot_action_prespawn_hash`] for actions in the [`OnFoot`]
    /// context.
    ///
    /// [`OnFoot`]: crate::contexts::OnFoot
    pub hash: u64,
    /// The local entity that owns the action (typically the player's
    /// predicted entity). Passed to lightyear as the `for_receiver`
    /// target so the prespawn pair survives even if the server-spawned
    /// twin arrives before this marker is processed.
    pub owner: Entity,
}

impl LocalActionPrespawnRequest {
    /// Creates a new prespawn request for the given hash + owner.
    #[inline]
    pub fn new(hash: u64, owner: Entity) -> Self {
        Self { hash, owner }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_deterministic() {
        let a = on_foot_action_prespawn_hash(0xDEAD_BEEF, OnFootAction::Move);
        let b = on_foot_action_prespawn_hash(0xDEAD_BEEF, OnFootAction::Move);
        assert_eq!(a, b);
    }

    #[test]
    fn different_actions_hash_differently() {
        let m = on_foot_action_prespawn_hash(42, OnFootAction::Move);
        let j = on_foot_action_prespawn_hash(42, OnFootAction::Jump);
        assert_ne!(m, j);
    }

    #[test]
    fn different_clients_hash_differently() {
        let a = on_foot_action_prespawn_hash(1, OnFootAction::Move);
        let b = on_foot_action_prespawn_hash(2, OnFootAction::Move);
        assert_ne!(a, b);
    }
}
