//! [`EntityPersister`] implementation for loose items.
//!
//! Collects every entity carrying a [`LooseItem`] component along with
//! its physics state, despawn-timer remainder, and pickup-cooldown
//! remainder; on load, respawns the entity with the same component set
//! that [`crate::spawn::spawn_loose_items`] uses.
//!
//! Bucketed by the chunk whose volume contains the entity's centre
//! point ([`ChunkPos::from(&Vec3)`]) so that unloading or reloading a
//! single chunk only touches its own items.

use bevy::prelude::*;
use bevy::time::Timer;
use dd40_core::chunk::ChunkPos;
use dd40_core::persistence::EntityPersister;
use dd40_item_core::active_item::ItemStack;
use dd40_loose_item_core::{DespawnTimer, LooseItem, PickupCooldown};
use dd40_physics_core::prelude::{PhysicsPosition, Velocity};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::spawn::loose_item_bundle;

/// Stable [`EntityPersister::kind`] for loose items.
///
/// Sidecar files written by older builds keyed on this string remain
/// loadable; renaming requires a migration.
pub const LOOSE_ITEM_KIND: &str = "loose_item_core.loose_item";

/// Versioned on-disk payload for a single loose item.
///
/// Stored inside [`dd40_core::persistence::PersistedEntity::payload`]
/// as bincode.  Adding a new variant is the upgrade path; old files
/// continue to decode through the existing variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LooseItemPayload {
    /// Initial format.
    V1(LooseItemPayloadV1),
}

/// Concrete v1 payload.  Holds the minimum needed to re-spawn a loose
/// item with the same physics + lifecycle state as it had at save time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LooseItemPayloadV1 {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub stack: ItemStack,
    /// Remaining lifetime in milliseconds.  Reconstructed as a fresh
    /// [`Timer`] with this duration.
    pub despawn_remaining_ms: u64,
    /// Remaining pickup cooldown in milliseconds.  Zero means the
    /// cooldown has already elapsed and the item is immediately
    /// pickable.
    pub cooldown_remaining_ms: u64,
}

/// Persister registered by [`crate::LooseItemsPlugin`] to keep loose
/// items alive across server restarts and (eventually) chunk
/// unload/reload cycles.
#[derive(Default)]
pub struct LooseItemPersister;

impl EntityPersister for LooseItemPersister {
    fn kind(&self) -> &'static str {
        LOOSE_ITEM_KIND
    }

    fn collect(&self, world: &mut World) -> Vec<(ChunkPos, Vec<u8>)> {
        let mut query = world.query::<(
            &PhysicsPosition,
            &Velocity,
            &LooseItem,
            &DespawnTimer,
            &PickupCooldown,
        )>();

        let mut out = Vec::new();
        for (pos, vel, item, despawn, cooldown) in query.iter(world) {
            let payload = LooseItemPayload::V1(LooseItemPayloadV1 {
                position: pos.0.to_array(),
                velocity: vel.0.to_array(),
                stack: item.stack,
                despawn_remaining_ms: remaining_ms(&despawn.0),
                cooldown_remaining_ms: remaining_ms(&cooldown.0),
            });
            let bytes = match bincode::serialize(&payload) {
                Ok(b) => b,
                Err(e) => {
                    error!("LooseItemPersister: failed to serialise payload: {e}");
                    continue;
                }
            };
            out.push((ChunkPos::from(&pos.0), bytes));
        }
        out
    }

    fn spawn(&self, world: &mut World, bytes: &[u8]) {
        let payload: LooseItemPayload = match bincode::deserialize(bytes) {
            Ok(p) => p,
            Err(e) => {
                error!("LooseItemPersister: failed to deserialise payload: {e}");
                return;
            }
        };
        let LooseItemPayload::V1(v) = payload;

        world.spawn(loose_item_bundle(
            Vec3::from_array(v.position),
            Vec3::from_array(v.velocity),
            v.stack,
            // A zero-length Timer would tick to `finished` immediately,
            // so clamp at 1 ms — close enough that gameplay won't
            // notice but guarantees the timer is constructible.
            Duration::from_millis(v.despawn_remaining_ms.max(1)),
            Duration::from_millis(v.cooldown_remaining_ms.max(1)),
        ));
    }
}

fn remaining_ms(timer: &Timer) -> u64 {
    timer
        .duration()
        .saturating_sub(timer.elapsed())
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spawn::loose_item_bundle;
    use dd40_item_core::registry::ItemId;
    use std::num::NonZero;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("non-zero literal")
    }

    fn stack() -> ItemStack {
        ItemStack::new(ItemId(0), nz(7))
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app
    }

    #[test]
    fn kind_is_stable() {
        assert_eq!(LooseItemPersister.kind(), "loose_item_core.loose_item");
    }

    #[test]
    fn collect_groups_by_owning_chunk() {
        let mut app = make_app();
        // Item near (3, 1, 5) → chunk (0, 0, 0).
        app.world_mut().spawn(loose_item_bundle(
            Vec3::new(3.0, 1.0, 5.0),
            Vec3::ZERO,
            stack(),
            Duration::from_secs(60),
            Duration::from_millis(500),
        ));
        // Item at (40, 1, 5) → chunk (2, 0, 0) for CHUNK_SIZE_X = 16.
        app.world_mut().spawn(loose_item_bundle(
            Vec3::new(40.0, 1.0, 5.0),
            Vec3::ZERO,
            stack(),
            Duration::from_secs(60),
            Duration::from_millis(500),
        ));

        let out = LooseItemPersister.collect(app.world_mut());
        assert_eq!(out.len(), 2);
        let chunks: Vec<ChunkPos> = out.iter().map(|(c, _)| *c).collect();
        assert!(chunks.contains(&ChunkPos::from(&Vec3::new(3.0, 1.0, 5.0))));
        assert!(chunks.contains(&ChunkPos::from(&Vec3::new(40.0, 1.0, 5.0))));
    }

    #[test]
    fn roundtrip_preserves_position_velocity_and_stack() {
        let mut app = make_app();
        let pos = Vec3::new(1.5, 2.5, 3.5);
        let vel = Vec3::new(0.0, 7.5, -1.0);
        app.world_mut().spawn(loose_item_bundle(
            pos,
            vel,
            stack(),
            Duration::from_secs(60),
            Duration::from_millis(500),
        ));

        let payloads = LooseItemPersister.collect(app.world_mut());
        assert_eq!(payloads.len(), 1);
        let bytes = payloads.into_iter().next().unwrap().1;

        let mut load_app = make_app();
        LooseItemPersister.spawn(load_app.world_mut(), &bytes);

        let mut q = load_app
            .world_mut()
            .query::<(&PhysicsPosition, &Velocity, &LooseItem)>();
        let entries: Vec<_> = q.iter(load_app.world()).collect();
        assert_eq!(entries.len(), 1);
        let (got_pos, got_vel, got_item) = entries[0];
        assert!((got_pos.0 - pos).length() < 1e-5);
        assert!((got_vel.0 - vel).length() < 1e-5);
        assert_eq!(got_item.stack, stack());
    }

    #[test]
    fn invalid_payload_does_not_panic() {
        let mut app = make_app();
        LooseItemPersister.spawn(app.world_mut(), &[0xff, 0x00, 0xff]);
        let mut q = app.world_mut().query::<&LooseItem>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }
}
