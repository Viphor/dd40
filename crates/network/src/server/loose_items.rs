//! Server-side replication for [`LooseItem`] entities.
//!
//! Loose items are spawned by `dd40_loose_items` without any
//! networking concern.  This module bridges those entities into
//! lightyear by:
//!
//! 1. [`add_loose_item_replication`] — observing `Added<LooseItem>` and
//!    inserting the [`Replicate`] bundle so every client interpolates
//!    the entity.
//! 2. [`sync_loose_item_position`] — copying the authoritative
//!    [`PhysicsPosition`] into the replicated [`LooseItemPosition`]
//!    every frame so client-side interpolation sees up-to-date data.

use bevy::prelude::*;
use dd40_loose_item_core::LooseItem;
use dd40_physics_core::prelude::PhysicsPosition;
use lightyear::prelude::{InterpolationTarget, NetworkTarget, Replicate};

use crate::protocol::LooseItemPosition;

/// Inserts the replication bundle on any newly spawned loose item so
/// every client receives an interpolated copy.
///
/// Runs in `PostUpdate` so it sees entities spawned during the same
/// frame (after [`LooseItemSet::Spawn`](
/// dd40_loose_item_core::system_sets::LooseItemSet::Spawn)).
pub fn add_loose_item_replication(
    mut commands: Commands,
    new_loose: Query<(Entity, &PhysicsPosition), (Added<LooseItem>, Without<Replicate>)>,
) {
    for (entity, pos) in &new_loose {
        commands.entity(entity).insert((
            LooseItemPosition(pos.0),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ));
    }
}

/// Mirrors the authoritative [`PhysicsPosition`] into the replicated
/// [`LooseItemPosition`] each frame.
pub fn sync_loose_item_position(
    mut q: Query<(&PhysicsPosition, &mut LooseItemPosition), With<LooseItem>>,
) {
    for (pos, mut replicated) in &mut q {
        if replicated.0 != pos.0 {
            replicated.0 = pos.0;
        }
    }
}
