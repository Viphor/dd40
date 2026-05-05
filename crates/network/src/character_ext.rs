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
    /// - [`ActionState<PlayerInput>`] so lightyear can buffer the controlling
    ///   client's inputs into it each tick.
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
        use crate::protocol::{NetworkCharacter, PlayerInput, PlayerPosition, PlayerRotation};
        use lightyear::prelude::{
            ControlledBy, InterpolationTarget, NetworkTarget, PredictionTarget, Replicate,
            input::native::ActionState,
        };

        self.add_extra(move |entity| {
            entity.insert((
                NetworkCharacter,
                ActionState::<PlayerInput>::default(),
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
/// `on_predicted_character_added` observer to attach the components required
/// for the client to drive its own predicted entity.
#[cfg(feature = "client")]
pub trait CharacterClientNetworkExt: Sized {
    /// Configures the character as the local player's predicted entity.
    ///
    /// Inserts:
    /// - [`InputMarker<PlayerInput>`](lightyear::prelude::input::native::InputMarker)
    ///   so lightyear knows this client controls the entity.
    /// - [`Player`](dd40_character_core::components::Player) marker.
    /// - [`CharacterPosition`](dd40_physics_core::prelude::CharacterPosition)
    ///   set explicitly to `initial_pos`, overriding the `Vec3::ZERO` default
    ///   that `on_add` would otherwise install before the replicated
    ///   `Transform` is applied.
    /// - [`PhysicsInterpolationData`] seeded so the first render frame shows
    ///   the entity at the spawn position.
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
        use crate::protocol::PlayerInput;
        use dd40_character_core::components::Player;
        use dd40_physics_core::prelude::CharacterPosition;
        use lightyear::prelude::input::native::InputMarker;

        self.add_extra(move |entity| {
            entity.insert((
                InputMarker::<PlayerInput>::default(),
                Player,
                CharacterPosition(initial_pos),
                PhysicsInterpolationData::new(initial_pos),
            ));
        });
        self
    }
}
