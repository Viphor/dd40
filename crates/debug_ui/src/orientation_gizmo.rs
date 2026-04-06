//! Minecraft-style orientation gizmo drawn in the bottom-right corner of the
//! screen.
//!
//! The gizmo shows three coloured axis arrows that rotate in sync with the
//! main camera, giving the player a constant read of which cardinal direction
//! they are facing:
//!
//! | Axis | Colour      | Label |
//! |------|-------------|-------|
//! | +X   | Red         | E     |
//! | −X   | Dark Red    | W     |
//! | +Y   | Green       | +Y    |
//! | −Y   | Dark Green  | -Y    |
//! | +Z   | Blue        | S     |
//! | −Z   | Dark Blue   | N     |
//!
//! # Implementation
//!
//! A dedicated [`Camera2d`] entity is spawned with a high `order` so it
//! composites on top of the 3-D scene.  It is assigned to a private
//! [`RenderLayers`] layer so it only sees the gizmo geometry and labels.
//!
//! Every frame [`draw_orientation_gizmo`] reads the main [`Camera3d`]
//! rotation, projects each axis direction into camera space, converts the
//! result to 2-D screen coordinates anchored in the bottom-right corner, and
//! draws lines + arrowheads via [`Gizmos`].  Cardinal labels are rendered as
//! [`Text2d`] entities that live on the same private layer and are repositioned
//! each frame to track their axis tip.

use bevy::{camera::visibility::RenderLayers, prelude::*};

// ── Constants ─────────────────────────────────────────────────────────────────

/// The [`RenderLayers`] layer index used exclusively for the overlay camera,
/// the gizmo lines, and the label [`Text2d`] entities.  Must not clash with
/// any layer used by the main scene.
const GIZMO_LAYER: usize = 16;

/// Side length (in logical pixels) of the square area reserved for the gizmo
/// in the bottom-right corner.
const VIEWPORT_SIZE: f32 = 100.0;

/// Margin from the window edges in logical pixels.
const VIEWPORT_MARGIN: f32 = 16.0;

/// Radius of the axis arrows in logical pixels (distance from anchor to tip).
const AXIS_RADIUS: f32 = 36.0;

/// Length of each arrowhead wing in logical pixels.
const ARROWHEAD_WING: f32 = 7.0;

// ── Gizmo config group ────────────────────────────────────────────────────────

/// Custom [`GizmoConfigGroup`] for the orientation gizmo so its line width and
/// render layer can be set independently of any other gizmos in the project.
#[derive(GizmoConfigGroup, Reflect, Default)]
pub struct OrientationGizmoConfig {}

// ── Marker components ─────────────────────────────────────────────────────────

/// Marks the dedicated 2-D overlay [`Camera2d`] used by the orientation gizmo.
#[derive(Component)]
struct GizmoCamera;

/// Placed on each axis-label [`Text2d`] entity so they can be queried and
/// repositioned every frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisLabel {
    /// Which half-axis this label belongs to.
    pub axis: HalfAxis,
}

// ── HalfAxis ──────────────────────────────────────────────────────────────────

/// One of the six directed half-axes shown in the orientation gizmo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalfAxis {
    /// +X — East
    PosX,
    /// −X — West
    NegX,
    /// +Y — Up
    PosY,
    /// −Y — Down
    NegY,
    /// +Z — South
    PosZ,
    /// −Z — North
    NegZ,
}

impl HalfAxis {
    const ALL: [HalfAxis; 6] = [
        HalfAxis::PosX,
        HalfAxis::NegX,
        HalfAxis::PosY,
        HalfAxis::NegY,
        HalfAxis::PosZ,
        HalfAxis::NegZ,
    ];

    /// World-space unit direction for this half-axis.
    fn direction(self) -> Vec3 {
        match self {
            HalfAxis::PosX => Vec3::X,
            HalfAxis::NegX => Vec3::NEG_X,
            HalfAxis::PosY => Vec3::Y,
            HalfAxis::NegY => Vec3::NEG_Y,
            HalfAxis::PosZ => Vec3::Z,
            HalfAxis::NegZ => Vec3::NEG_Z,
        }
    }

    /// Display colour for this half-axis.
    fn color(self) -> Color {
        match self {
            HalfAxis::PosX => Color::srgb(0.93, 0.23, 0.23),
            HalfAxis::NegX => Color::srgb(0.50, 0.10, 0.10),
            HalfAxis::PosY => Color::srgb(0.23, 0.93, 0.23),
            HalfAxis::NegY => Color::srgb(0.10, 0.50, 0.10),
            HalfAxis::PosZ => Color::srgb(0.30, 0.50, 0.93),
            HalfAxis::NegZ => Color::srgb(0.10, 0.20, 0.50),
        }
    }

    /// Short text label shown near the axis tip.
    fn label(self) -> &'static str {
        match self {
            HalfAxis::PosX => "E",
            HalfAxis::NegX => "W",
            HalfAxis::PosY => "+Y",
            HalfAxis::NegY => "-Y",
            HalfAxis::PosZ => "S",
            HalfAxis::NegZ => "N",
        }
    }

    /// `true` for the three canonical (+) half-axes.
    fn is_positive(self) -> bool {
        matches!(self, HalfAxis::PosX | HalfAxis::PosY | HalfAxis::PosZ)
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Adds the Minecraft-style orientation gizmo to the bottom-right corner of
/// the screen.
///
/// This plugin is included automatically by [`DebugUiPlugin`].
///
/// [`DebugUiPlugin`]: crate::DebugUiPlugin
pub struct OrientationGizmoPlugin;

impl Plugin for OrientationGizmoPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<OrientationGizmoConfig>()
            .add_systems(
                Startup,
                (setup_gizmo_camera, spawn_axis_labels, configure_gizmo_layer),
            )
            .add_systems(Update, draw_orientation_gizmo);
    }
}

// ── Startup systems ───────────────────────────────────────────────────────────

/// Spawns the dedicated 2-D overlay camera that renders only the gizmo layer.
///
/// `order: 1` means it composites on top of the default 3-D camera (`order: 0`).
fn setup_gizmo_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("GizmoOverlayCamera"),
        GizmoCamera,
        Camera2d,
        Camera {
            // Render after the main 3-D camera so the overlay appears on top.
            order: 1,
            // Do NOT clear the colour buffer — we want to see the 3-D scene
            // behind the gizmo.
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(GIZMO_LAYER),
    ));
}

/// Points the `OrientationGizmoConfig` gizmo group at the private overlay
/// layer so the lines only appear through the [`GizmoCamera`].
fn configure_gizmo_layer(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<OrientationGizmoConfig>();
    config.render_layers = RenderLayers::layer(GIZMO_LAYER);
    config.line.width = 2.5;
}

/// Spawns one [`Text2d`] entity per half-axis, assigned to the gizmo render
/// layer.  Positions are updated every frame in [`draw_orientation_gizmo`].
fn spawn_axis_labels(mut commands: Commands) {
    for &axis in &HalfAxis::ALL {
        commands.spawn((
            Name::new(format!("GizmoLabel {:?}", axis)),
            AxisLabel { axis },
            Text2d::new(axis.label()),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(axis.color()),
            // Start far off-screen; repositioned each frame.
            Transform::from_xyz(-99999.0, -99999.0, 1.0),
            // Only visible through the overlay camera.
            RenderLayers::layer(GIZMO_LAYER),
        ));
    }
}

// ── Update system ─────────────────────────────────────────────────────────────

/// Draws the orientation gizmo arrows and repositions the cardinal labels.
///
/// # Projection
///
/// 1. Read the main [`Camera3d`] rotation.
/// 2. Apply its inverse to each world-space axis direction → camera-space
///    direction.
/// 3. The camera-space X and Y components are the 2-D screen offsets (right /
///    up); Z is used only for depth-sorting.
/// 4. Scale the offsets by [`AXIS_RADIUS`] and add the anchor position
///    (bottom-right corner expressed in Bevy's 2-D world space where the
///    origin is the window centre).
/// 5. Draw via [`Gizmos<OrientationGizmoConfig>`], which targets only the
///    overlay camera thanks to [`configure_gizmo_layer`].
fn draw_orientation_gizmo(
    mut gizmos: Gizmos<OrientationGizmoConfig>,
    camera_query: Query<&Transform, With<Camera3d>>,
    windows: Query<&Window>,
    mut label_query: Query<(&AxisLabel, &mut Transform), Without<Camera3d>>,
) {
    let Ok(cam_transform) = camera_query.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let win_w = window.resolution.width();
    let win_h = window.resolution.height();

    // Bevy's 2-D world space: origin at window centre, +X right, +Y up.
    // Place the gizmo anchor at the centre of the bottom-right reserved area.
    let anchor = Vec2::new(
        win_w * 0.5 - VIEWPORT_SIZE * 0.5 - VIEWPORT_MARGIN,
        -(win_h * 0.5 - VIEWPORT_SIZE * 0.5 - VIEWPORT_MARGIN),
    );

    // Inverse rotation: maps world-space directions into camera space.
    let inv_rot = cam_transform.rotation.inverse();

    // Project all six half-axes into 2-D and collect with depth for sorting.
    let mut projected: Vec<(HalfAxis, Vec2, f32)> = HalfAxis::ALL
        .iter()
        .map(|&axis| {
            let cam_dir = inv_rot * axis.direction();
            // cam_dir.x → screen right (+X), cam_dir.y → screen up (+Y)
            // cam_dir.z → away from viewer in Bevy's right-handed camera space
            let tip = anchor + Vec2::new(cam_dir.x, cam_dir.y) * AXIS_RADIUS;
            (axis, tip, cam_dir.z)
        })
        .collect();

    // Sort back-to-front (most-negative Z = closest to viewer, drawn last /
    // on top) for a convincing pseudo-3-D look.
    projected.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Small white dot at the gizmo origin.
    gizmos.circle_2d(anchor, 3.0, Color::WHITE);

    for &(axis, tip, depth) in &projected {
        // Axes pointing away from the viewer get a low alpha to look recessed.
        let alpha = if depth > 0.0 { 0.30_f32 } else { 1.0_f32 };
        let color = color_with_alpha(axis.color(), alpha);

        // Shaft.
        gizmos.line_2d(anchor, tip, color);

        if axis.is_positive() {
            // Arrowhead on positive axes.
            draw_arrowhead(&mut gizmos, anchor, tip, color);
        } else {
            // Small circle on negative axes.
            gizmos.circle_2d(tip, 3.5, color);
        }
    }

    // Reposition label entities to track their axis tips.
    for (label, mut transform) in &mut label_query {
        if let Some(&(_, tip, _)) = projected.iter().find(|(a, _, _)| *a == label.axis) {
            let dir = (tip - anchor).normalize_or_zero();
            // Nudge the label slightly past the tip to avoid overlapping the arrowhead.
            let label_pos = tip + dir * 12.0;
            transform.translation = Vec3::new(label_pos.x, label_pos.y, 1.0);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Draws a two-wing arrowhead at `tip` pointing away from `base`.
fn draw_arrowhead(
    gizmos: &mut Gizmos<OrientationGizmoConfig>,
    base: Vec2,
    tip: Vec2,
    color: Color,
) {
    let forward = (tip - base).normalize_or_zero();
    let right = Vec2::new(-forward.y, forward.x);

    let root = tip - forward * ARROWHEAD_WING;
    gizmos.line_2d(tip, root + right * ARROWHEAD_WING * 0.55, color);
    gizmos.line_2d(tip, root - right * ARROWHEAD_WING * 0.55, color);
}

/// Returns `color` with its alpha replaced by `alpha`.
fn color_with_alpha(color: Color, alpha: f32) -> Color {
    let l = color.to_linear();
    Color::LinearRgba(LinearRgba::new(l.red, l.green, l.blue, alpha))
}
