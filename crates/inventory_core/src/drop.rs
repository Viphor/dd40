//! [`DropItems`] — request to spawn item entities at a world position.
//!
//! `DropItems` is the single seam between systems that **decide** items
//! should be dropped (a block being mined, a character dying, a chest
//! breaking open) and the future system that **spawns** the item
//! entities that represent those drops in the world.
//!
//! Sending this message must not require knowing whether the actual
//! item-entity spawner is loaded — when no consumer is registered, the
//! message is simply dropped on the next frame.  This lets crates like
//! `dd40_loot` produce drops today even though item-entity rendering
//! lands later.
//!
//! # Authority
//!
//! `DropItems` is server-authoritative.  Clients should never write
//! this message directly: in networked play the server processes
//! drops and replicates the resulting item entities back to clients.
//! The message itself is registered in `dd40_inventory_core` (the
//! foundation crate) only so both server and client agree on the type.

use bevy::math::Vec3;
use bevy::prelude::Message;

use dd40_item_core::active_item::ItemStack;

/// Request to spawn item entities for one or more stacks at a world
/// position.
///
/// # Fields
///
/// - `origin` — world-space position at which the items should appear,
///   typically the centre of the source block or the world position of
///   the dying entity.
/// - `velocity` — initial velocity applied to the spawned entities.
///   Pass [`Vec3::ZERO`] for "drop in place".  Consumers may add a
///   small random scatter on top — that randomness should originate
///   from [`GameRng`][dd40_rng::GameRng] so it stays deterministic and
///   server-authoritative.
/// - `stacks` — the stacks to spawn.  Empty `stacks` is a no-op and
///   consumers must tolerate it.  How many entities the consumer
///   spawns per stack (one per stack vs one per item) is the
///   consumer's choice, not part of this contract.
///
/// # Why this is a `Message`, not an `Event`
///
/// Drops are produced as a side-effect of validated chunk changes and
/// character death paths, and consumed by a separate item-spawn
/// system.  That is the canonical use-case for a queued message:
/// producer and consumer are decoupled, the timing is "next frame is
/// fine", and there is no need to react before the next tick.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct DropItems {
    /// World-space position at which the items should appear.
    pub origin: Vec3,
    /// Initial velocity applied to the spawned entities.
    pub velocity: Vec3,
    /// Stacks to spawn.  Empty is a tolerated no-op.
    pub stacks: Vec<ItemStack>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use dd40_item_core::registry::ItemId;
    use std::num::NonZero;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("nz literal must be non-zero")
    }

    /// `DropItems` is a Bevy `Message`: the producer writes via
    /// [`MessageWriter`] and the consumer drains via [`MessageReader`]
    /// on a later (or same) tick.
    #[test]
    fn writer_reader_round_trip() {
        let mut app = App::new();
        app.add_message::<DropItems>();

        let msg = DropItems {
            origin: Vec3::new(1.5, 64.0, -3.0),
            velocity: Vec3::new(0.0, 1.0, 0.0),
            stacks: vec![ItemStack::new(ItemId(7), nz(3))],
        };

        let to_write = msg.clone();
        fn produce_factory(
            msg: DropItems,
        ) -> impl FnMut(MessageWriter<DropItems>) + Send + Sync + 'static {
            move |mut w: MessageWriter<DropItems>| {
                w.write(msg.clone());
            }
        }

        #[derive(Resource, Default)]
        struct Captured(Vec<DropItems>);
        app.init_resource::<Captured>();
        app.add_systems(
            Update,
            (
                produce_factory(to_write),
                (|mut r: MessageReader<DropItems>, mut captured: ResMut<Captured>| {
                    for ev in r.read() {
                        captured.0.push(ev.clone());
                    }
                }),
            )
                .chain(),
        );

        app.update();
        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0], msg);
    }

    /// An empty `stacks` vec is a tolerated no-op; the message must
    /// still round-trip without panicking.
    #[test]
    fn empty_stacks_round_trip() {
        let mut app = App::new();
        app.add_message::<DropItems>();

        #[derive(Resource, Default)]
        struct Count(usize);
        app.init_resource::<Count>();
        app.add_systems(
            Update,
            (
                (|mut w: MessageWriter<DropItems>| {
                    w.write(DropItems {
                        origin: Vec3::ZERO,
                        velocity: Vec3::ZERO,
                        stacks: Vec::new(),
                    });
                }),
                (|mut r: MessageReader<DropItems>, mut count: ResMut<Count>| {
                    for _ in r.read() {
                        count.0 += 1;
                    }
                }),
            )
                .chain(),
        );

        app.update();
        assert_eq!(app.world().resource::<Count>().0, 1);
    }
}
