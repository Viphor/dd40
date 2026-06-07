//! Client-side bridge for replicated [`LooseItem`] entities.
//!
//! Lightyear spawns a separate `Interpolated` entity that carries the
//! smoothed [`LooseItemPosition`].  Renderers (e.g.
//! `dd40_loose_item_render`) attach their visual to the `Interpolated`
//! entity using the regular [`LooseItem`] component, so we copy the
//! interpolated position into `Transform.translation` every frame.

use bevy::prelude::*;

use dd40_loose_item_core::LooseItem;
use lightyear::prelude::Interpolated;

use crate::protocol::LooseItemPosition;

/// Ensures every interpolated loose-item entity has a [`Transform`] +
/// [`GlobalTransform`] so renderers can hang children off it via the
/// transform hierarchy.  Lightyear only replicates the components we
/// register, so the transform stack is not present by default on the
/// interpolated copy.
///
/// Visibility is intentionally omitted here — it belongs to the render
/// layer, not to networking.  `dd40_loose_item_render` (or whatever
/// renderer is active) is responsible for adding it.
pub fn ensure_loose_item_transform(
    mut commands: Commands,
    new: Query<Entity, (With<LooseItem>, With<Interpolated>, Without<Transform>)>,
) {
    for entity in &new {
        commands.entity(entity).insert((
            Transform::default(),
            GlobalTransform::default(),
        ));
    }
}

/// Copies the interpolated [`LooseItemPosition`] into [`Transform`]
/// each frame so the visual follows server-authoritative motion.
pub fn sync_loose_item_position_to_transform(
    mut q: Query<(&LooseItemPosition, &mut Transform), With<Interpolated>>,
) {
    for (pos, mut transform) in &mut q {
        transform.translation = pos.0;
    }
}
