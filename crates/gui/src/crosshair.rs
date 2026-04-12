//! Crosshair UI element drawn at the centre of the screen.
//!
//! The crosshair is a simple `+` shape built from two thin white [`Node`]
//! rectangles (one horizontal, one vertical) that are absolutely positioned
//! over a full-screen transparent container.  A small dark outline is
//! simulated by making the bars very slightly wider/taller with a dark shadow
//! node behind each bar.
//!
//! # Registration
//!
//! This module is wired up by [`crate::plugin::GuiPlugin`].  You do not need
//! to call anything here directly.

use bevy::prelude::*;

// ── Dimensions ────────────────────────────────────────────────────────────────

/// Total length of each crosshair arm, in logical pixels.
const ARM_LENGTH: f32 = 10.0;
/// Thickness of each bar, in logical pixels.
const BAR_THICKNESS: f32 = 2.0;
/// Extra pixels added on each side of a bar for the dark outline.
const OUTLINE_PADDING: f32 = 1.0;
/// Colour of the crosshair bars.
const BAR_COLOR: Color = Color::Srgba(Srgba {
    red: 0.7,
    green: 0.7,
    blue: 0.7,
    alpha: 1.0,
});
/// Colour of the dark outline behind each bar.
const OUTLINE_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);

// ── Marker component ──────────────────────────────────────────────────────────

/// Marks the root node of the crosshair UI hierarchy so it can be queried or
/// despawned independently of other UI elements.
#[derive(Component)]
pub struct CrosshairRoot;

// ── Spawn system ──────────────────────────────────────────────────────────────

/// Spawns the crosshair UI hierarchy.
///
/// The hierarchy looks like:
///
/// ```text
/// CrosshairRoot  (full-screen transparent overlay, pointer pass-through)
///   └─ centre container  (flexbox, centres children)
///        ├─ horizontal outline  (slightly taller dark rect)
///        │    └─ horizontal bar  (white rect)
///        └─ vertical outline    (slightly wider dark rect, absolute)
///             └─ vertical bar   (white rect)
/// ```
///
/// Both bars are centred on the same point via absolute positioning so they
/// overlap to form a `+`.
pub fn spawn_crosshair(mut commands: Commands) {
    commands
        .spawn((
            Name::new("CrosshairRoot"),
            CrosshairRoot,
            Node {
                // Cover the entire window so we can centre children freely.
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                // Centre the crosshair both horizontally and vertically.
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                // Allow pointer events to fall through to the 3-D scene.
                ..default()
            },
            // Fully transparent — this node is layout-only.
            BackgroundColor(Color::NONE),
            // Do not block picking / interaction events.
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // ── Centre pivot ─────────────────────────────────────────────────
            // A zero-size container that acts as the shared anchor point for
            // both bars.  Children use absolute positioning relative to it.
            root.spawn(Node {
                position_type: PositionType::Relative,
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                // We need absolute children to overflow, so do not clip.
                overflow: Overflow::visible(),
                ..default()
            })
            .with_children(|pivot| {
                // ── Horizontal bar ───────────────────────────────────────────
                let h_outline_w = ARM_LENGTH * 2.0 + OUTLINE_PADDING * 2.0;
                let h_outline_h = BAR_THICKNESS + OUTLINE_PADDING * 2.0;

                // Dark outline (slightly larger, behind the white bar).
                pivot
                    .spawn((
                        Name::new("CrosshairHOutline"),
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(h_outline_w),
                            height: Val::Px(h_outline_h),
                            // Offset so it is perfectly centred on the pivot.
                            left: Val::Px(-h_outline_w / 2.0),
                            top: Val::Px(-h_outline_h / 2.0),
                            // Centre the white child bar inside.
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(OUTLINE_COLOR),
                    ))
                    .with_children(|outline| {
                        // White bar on top of the outline.
                        outline.spawn((
                            Name::new("CrosshairHBar"),
                            Node {
                                width: Val::Px(ARM_LENGTH * 2.0),
                                height: Val::Px(BAR_THICKNESS),
                                ..default()
                            },
                            BackgroundColor(BAR_COLOR),
                        ));
                    });

                // ── Vertical bar ─────────────────────────────────────────────
                let v_outline_w = BAR_THICKNESS + OUTLINE_PADDING * 2.0;
                let v_outline_h = ARM_LENGTH * 2.0 + OUTLINE_PADDING * 2.0;

                pivot
                    .spawn((
                        Name::new("CrosshairVOutline"),
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(v_outline_w),
                            height: Val::Px(v_outline_h),
                            left: Val::Px(-v_outline_w / 2.0),
                            top: Val::Px(-v_outline_h / 2.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(OUTLINE_COLOR),
                    ))
                    .with_children(|outline| {
                        outline.spawn((
                            Name::new("CrosshairVBar"),
                            Node {
                                width: Val::Px(BAR_THICKNESS),
                                height: Val::Px(ARM_LENGTH * 2.0),
                                ..default()
                            },
                            BackgroundColor(BAR_COLOR),
                        ));
                    });
            });
        });
}
