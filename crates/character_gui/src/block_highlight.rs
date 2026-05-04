//! Targeted-block highlight gizmo (and, in a later slice, mining break overlay).
//!
//! This module owns every visual that depends on a [`Player`]'s
//! [`TargetedBlock`].  Rendering is gizmo-based so the only Bevy feature we
//! need is `bevy_gizmos`; no asset pipeline required.

use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_character_core::targeted_block::TargetedBlock;

/// Render-only configuration for the targeted-block highlight and (future)
/// mining break overlay.
///
/// Lives in `dd40_character_gui` so the headless server never needs to know
/// about it.  Override by inserting your own `BlockHighlightConfig` resource
/// before [`crate::plugin::CharacterGuiPlugin`] runs its startup.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct BlockHighlightConfig {
    /// Colour of the wireframe cuboid drawn around the targeted block.
    pub outline_color: Color,
    /// Colour of the inner break-overlay cube at `progress = 0.0`.
    pub mining_color_start: Color,
    /// Colour of the inner break-overlay cube at `progress = 1.0`.
    pub mining_color_end: Color,
}

impl Default for BlockHighlightConfig {
    fn default() -> Self {
        Self {
            outline_color: Color::BLACK,
            mining_color_start: Color::srgb(0.2, 0.2, 0.2),
            mining_color_end: Color::srgb(0.95, 0.95, 0.95),
        }
    }
}

/// Draws a wireframe cuboid gizmo around the local player's currently
/// targeted block.
///
/// Filters on [`Player`] so only the local character's target is drawn,
/// even when remote characters also have a [`TargetedBlock`].
pub(crate) fn draw_targeted_block_highlight(
    targeted_query: Query<&TargetedBlock, With<Player>>,
    config: Res<BlockHighlightConfig>,
    mut gizmos: Gizmos,
) {
    let Some(targeted) = targeted_query.iter().next() else {
        return;
    };
    let Some(pos) = targeted.pos else { return };
    let center = Vec3::new(
        pos.x as f32 + 0.5,
        pos.y as f32 + 0.5,
        pos.z as f32 + 0.5,
    );
    const EPSILON: f32 = 0.0002;
    let size = Vec3::splat(1.0 + EPSILON * 2.0);
    gizmos.cube(
        Transform::from_translation(center).with_scale(size),
        config.outline_color,
    );
}
