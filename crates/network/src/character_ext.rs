//! Extension traits adding networking capabilities to a character builder.
//!
//! These traits are implemented as blanket impls on any
//! [`AddExtra`](dd40_core::builder_extra::AddExtra) type, so they apply to
//! [`CharacterBuilder`](dd40_character_core::builder::CharacterBuilder) without
//! requiring `dd40_network` to depend on `dd40_character_core` directly via the
//! builder type — the dependency is on the protocol abstraction.
//!
//! # Example
//!
//! ```rust,no_run
//! # use bevy::prelude::*;
//! # use dd40_character_core::builder::CharacterBuilder;
//! # #[cfg(feature = "server")]
//! # use dd40_network::character_ext::CharacterServerNetworkExt;
//! # use dd40_physics_core::character_ext::CharacterPhysicsExt;
//! # fn example(mut commands: Commands, owner: Entity) {
//! # #[cfg(feature = "server")]
//! # let _ = {
//! CharacterBuilder::new("Alice")
//!     .transform(Transform::from_xyz(0.0, 74.0, 0.0))
//!     .with_physics()
//!     .with_server_replication(
//!         lightyear::prelude::PeerId::Netcode(1),
//!         Vec3::new(0.0, 74.0, 0.0),
//!         owner,
//!     )
//!     .spawn(&mut commands);
//! # };
//! # }
//! ```

use bevy::prelude::*;
use dd40_core::builder_extra::AddExtra;

/// Server-side networking capability for a character builder.
///
/// Adds the lightyear components required for server-authoritative
/// replication of a character entity, including prediction routing for the
/// controlling client and snapshot interpolation for everyone else.
#[cfg(feature = "server")]
pub trait CharacterServerNetworkExt: Sized {
    /// Marks the character for full server-authoritative replication.
    ///
    /// Inserts:
    /// - [`NetworkCharacter`](crate::protocol::NetworkCharacter) marker.
    /// - [`OnFoot`](dd40_input_core::contexts::OnFoot) input context, so
    ///   lightyear's BEI integration can target this entity with the
    ///   controlling client's replicated action set.
    /// - [`PlayerPosition`](crate::protocol::PlayerPosition) and
    ///   [`PlayerRotation`](crate::protocol::PlayerRotation), seeded from
    ///   `spawn_pos`.
    /// - [`Replicate`](lightyear::prelude::Replicate) targeting all clients.
    /// - [`PredictionTarget`](lightyear::prelude::PredictionTarget) targeting
    ///   the controlling client only (`client_id`).
    /// - [`InterpolationTarget`](lightyear::prelude::InterpolationTarget)
    ///   targeting every other client.
    /// - [`ControlledBy`](lightyear::prelude::ControlledBy) so the entity
    ///   despawns when the owning connection drops.
    ///
    /// `Action<T>` entities are **not** spawned here — the controlling
    /// client spawns them and lightyear replicates them up to the server
    /// (see `with_predicted_local_player`).
    ///
    /// # Parameters
    ///
    /// - `client_id` — the lightyear peer id of the controlling client.
    /// - `spawn_pos` — the initial world-space position.
    /// - `owner` — the connection entity (the `Entity` carrying `Connected`)
    ///   that owns this character.
    fn with_server_replication(
        self,
        client_id: lightyear::prelude::PeerId,
        spawn_pos: Vec3,
        owner: Entity,
    ) -> Self;
}

#[cfg(feature = "server")]
impl<T: AddExtra> CharacterServerNetworkExt for T {
    fn with_server_replication(
        mut self,
        client_id: lightyear::prelude::PeerId,
        spawn_pos: Vec3,
        owner: Entity,
    ) -> Self {
        use crate::protocol::{NetworkCharacter, PlayerPosition, PlayerRotation};
        use dd40_input_core::contexts::OnFoot;
        use lightyear::prelude::{
            ControlledBy, InterpolationTarget, NetworkTarget, PredictionTarget, Replicate,
        };

        self.add_extra(move |entity| {
            entity.insert((
                NetworkCharacter,
                OnFoot,
                PlayerPosition::from_vec3(spawn_pos),
                PlayerRotation::new(0.0, 0.0),
                Replicate::to_clients(NetworkTarget::All),
                PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
                InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
                ControlledBy {
                    owner,
                    lifetime: Default::default(),
                },
            ));
        });
        self
    }
}

/// Client-side networking capability for a predicted local-player character.
///
/// This is the counterpart to [`CharacterServerNetworkExt`] used inside the
/// `on_network_character_added` observer to attach the components required
/// for the client to drive its own predicted entity.
#[cfg(feature = "client")]
pub trait CharacterClientNetworkExt: Sized {
    /// Configures the character as the local player's predicted entity.
    ///
    /// Inserts:
    /// - [`OnFoot`](dd40_input_core::contexts::OnFoot) input context (no-op
    ///   if replication already delivered it).
    /// - [`InputMarker<OnFoot>`](lightyear::input::bei::prelude::InputMarker)
    ///   so lightyear treats this client as the controller. Lightyear's
    ///   observers propagate the marker to every related Action entity.
    /// - [`Player`](dd40_character_core::components::Player) marker.
    /// - [`PhysicsInterpolationData`] seeded so the first render frame shows
    ///   the entity at the spawn position.
    ///
    /// Action entities + bindings are spawned by `dd40_player_input` when
    /// it sees a new [`Player`](dd40_character_core::components::Player)
    /// entity, so the network layer stays independent of the input crate.
    ///
    /// # Parameters
    ///
    /// - `initial_pos` — the spawn position read from the replicated
    ///   `PlayerPosition`.
    fn with_predicted_local_player(self, initial_pos: Vec3) -> Self;
}

#[cfg(feature = "client")]
impl<T: AddExtra> CharacterClientNetworkExt for T {
    fn with_predicted_local_player(mut self, initial_pos: Vec3) -> Self {
        use crate::client::character::PhysicsInterpolationData;
        use dd40_character_core::components::Player;
        use dd40_input_core::contexts::OnFoot;
        use lightyear::input::bei::prelude::InputMarker;

        self.add_extra(move |entity| {
            entity.insert((
                OnFoot,
                InputMarker::<OnFoot>::default(),
                Player,
                PhysicsInterpolationData::new(initial_pos),
            ));
        });
        self
    }
}
