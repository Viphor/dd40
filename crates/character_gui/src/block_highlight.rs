//! Targeted-block highlight gizmo (and, in a later slice, mining break overlay).
//!
//! This module owns every visual that depends on a [`Player`]'s
//! [`TargetedBlock`].  Rendering is gizmo-based so the only Bevy feature we
//! need is `bevy_gizmos`; no asset pipeline required.

use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_character_core::mining_state::MiningState;
use dd40_character_core::targeted_block::TargetedBlock;

/// Render-only configuration for the targeted-block highlight and the
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
    /// Edge length (in blocks) of the inner break-overlay cube at
    /// `progress = 0.0`.  Should be `<= 1.0` so it fits inside the block.
    pub mining_scale_start: f32,
    /// Edge length (in blocks) of the inner break-overlay cube at
    /// `progress = 1.0`.  Smaller = "block has been chipped down further".
    pub mining_scale_end: f32,
}

impl Default for BlockHighlightConfig {
    fn default() -> Self {
        Self {
            outline_color: Color::BLACK,
            mining_color_start: Color::srgb(0.2, 0.2, 0.2),
            mining_color_end: Color::srgb(0.95, 0.95, 0.95),
            mining_scale_start: 1.0,
            mining_scale_end: 0.1,
        }
    }
}

/// Returns the `(scale, colour)` of the inner break-overlay cube for a
/// given mining progress in `[0.0, 1.0]`.
///
/// `progress` is clamped, so values produced by buggy upstream systems do
/// not flip the cube inside out or produce out-of-gamut colours.
///
/// Pure function — no ECS access — so the easing curve can be unit-tested
/// without spinning up an [`App`].
pub fn break_overlay_for_progress(progress: f32, cfg: &BlockHighlightConfig) -> (Vec3, Color) {
    let p = progress.clamp(0.0, 1.0);
    let scale = cfg.mining_scale_start + (cfg.mining_scale_end - cfg.mining_scale_start) * p;
    let start = cfg.mining_color_start.to_linear();
    let end = cfg.mining_color_end.to_linear();
    let colour = LinearRgba {
        red: start.red + (end.red - start.red) * p,
        green: start.green + (end.green - start.green) * p,
        blue: start.blue + (end.blue - start.blue) * p,
        alpha: start.alpha + (end.alpha - start.alpha) * p,
    };
    (Vec3::splat(scale), Color::LinearRgba(colour))
}

/// Draws the targeted-block outline and, while the local player is mining,
/// an inner cube whose scale and colour interpolate with mining progress.
///
/// Filters on [`Player`] so only the local character's visuals are drawn,
/// even when remote characters also have a [`TargetedBlock`].
pub(crate) fn draw_targeted_block_highlight(
    targeted_query: Query<(&TargetedBlock, Option<&MiningState>), With<Player>>,
    config: Res<BlockHighlightConfig>,
    mut gizmos: Gizmos,
) {
    let Some((targeted, mining)) = targeted_query.iter().next() else {
        return;
    };
    let Some(pos) = targeted.pos else { return };
    let center = Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5);
    const EPSILON: f32 = 0.0002;
    let outline_size = Vec3::splat(1.0 + EPSILON * 2.0);
    gizmos.cube(
        Transform::from_translation(center).with_scale(outline_size),
        config.outline_color,
    );

    if let Some(MiningState::Mining {
        pos: mining_pos,
        progress,
        ..
    }) = mining
    {
        // Only draw the break overlay if the player is actually mining the
        // block they are currently looking at.  Otherwise, a finished mine
        // still in `Mining` state could ghost-draw on a different block.
        if *mining_pos == pos {
            let (scale, colour) = break_overlay_for_progress(*progress, &config);
            gizmos.cube(
                Transform::from_translation(center).with_scale(scale),
                colour,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} !~= {b}");
    }

    fn approx_color(a: Color, b: Color) {
        let la = a.to_linear();
        let lb = b.to_linear();
        approx_eq(la.red, lb.red);
        approx_eq(la.green, lb.green);
        approx_eq(la.blue, lb.blue);
    }

    #[test]
    fn break_overlay_at_zero_uses_start() {
        let cfg = BlockHighlightConfig::default();
        let (scale, colour) = break_overlay_for_progress(0.0, &cfg);
        approx_eq(scale.x, cfg.mining_scale_start);
        approx_color(colour, cfg.mining_color_start);
    }

    #[test]
    fn break_overlay_at_one_uses_end() {
        let cfg = BlockHighlightConfig::default();
        let (scale, colour) = break_overlay_for_progress(1.0, &cfg);
        approx_eq(scale.x, cfg.mining_scale_end);
        approx_color(colour, cfg.mining_color_end);
    }

    #[test]
    fn break_overlay_at_half_is_midpoint() {
        let cfg = BlockHighlightConfig::default();
        let (scale, _) = break_overlay_for_progress(0.5, &cfg);
        approx_eq(
            scale.x,
            (cfg.mining_scale_start + cfg.mining_scale_end) * 0.5,
        );
    }

    #[test]
    fn break_overlay_clamps_negative_progress() {
        let cfg = BlockHighlightConfig::default();
        let (scale, _) = break_overlay_for_progress(-1.0, &cfg);
        approx_eq(scale.x, cfg.mining_scale_start);
    }

    #[test]
    fn break_overlay_clamps_over_one_progress() {
        let cfg = BlockHighlightConfig::default();
        let (scale, _) = break_overlay_for_progress(2.5, &cfg);
        approx_eq(scale.x, cfg.mining_scale_end);
    }

    #[test]
    fn break_overlay_produces_uniform_scale() {
        let cfg = BlockHighlightConfig::default();
        let (scale, _) = break_overlay_for_progress(0.7, &cfg);
        approx_eq(scale.x, scale.y);
        approx_eq(scale.y, scale.z);
    }
}
