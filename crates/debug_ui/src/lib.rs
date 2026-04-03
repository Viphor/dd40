use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::custom::{DebugUiElementRoot, spawn_custom_debug_ui, update_custom_debug_ui};

pub mod custom;

/// Plugin that provides debug UI elements.
/// Currently displays an FPS counter in the top-left corner.
pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup_debug_ui)
            .add_systems(Update, (spawn_custom_debug_ui, update_custom_debug_ui))
            .add_systems(Update, update_fps_text);
    }
}

/// Marker component for the FPS counter text.
#[derive(Component)]
struct FpsText;

/// Marker component for the loaded blocks counter text.
#[derive(Component)]
struct LoadedBlocksText;

/// Marker component for the rendered blocks counter text.
#[derive(Component)]
struct RenderedBlocksText;

/// Marker component for the culling efficiency text.
#[derive(Component)]
struct CullingEfficiencyText;

/// Sets up the debug UI elements.
fn setup_debug_ui(mut commands: Commands) {
    // Root node for debug UI
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::FlexStart,
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        })
        .with_children(|parent| {
            // Container for all debug text
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|parent| {
                    // FPS counter text
                    parent.spawn((
                        Text::new("FPS: --"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.0, 1.0, 0.0)),
                        Node {
                            position_type: PositionType::Relative,
                            ..default()
                        },
                        FpsText,
                    ));

                    // Loaded blocks counter text
                    parent.spawn((
                        Text::new("Loaded Blocks: --"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 1.0)),
                        Node {
                            position_type: PositionType::Relative,
                            ..default()
                        },
                        LoadedBlocksText,
                    ));

                    // Rendered blocks counter text
                    parent.spawn((
                        Text::new("Rendered Blocks: --"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.7, 0.0)),
                        Node {
                            position_type: PositionType::Relative,
                            ..default()
                        },
                        RenderedBlocksText,
                    ));

                    // Culling efficiency text
                    parent.spawn((
                        Text::new("Culling: --%"),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.0, 1.0, 1.0)),
                        Node {
                            position_type: PositionType::Relative,
                            ..default()
                        },
                        CullingEfficiencyText,
                    ));

                    // Container for custom debug UI elements
                    parent.spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(5.0),
                            ..default()
                        },
                        DebugUiElementRoot,
                    ));
                });
        });
}

/// Updates the FPS counter text every frame.
fn update_fps_text(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<(&mut Text, &mut TextColor), With<FpsText>>,
) {
    for (mut text, mut color) in &mut query {
        if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
            if let Some(value) = fps.smoothed() {
                **text = format!("FPS: {:.0}", value);

                // Color-code based on performance
                color.0 = if value >= 60.0 {
                    Color::srgb(0.0, 1.0, 0.0) // Green for good FPS
                } else if value >= 30.0 {
                    Color::srgb(1.0, 1.0, 0.0) // Yellow for moderate FPS
                } else {
                    Color::srgb(1.0, 0.0, 0.0) // Red for low FPS
                };
            }
        }
    }
}

/// Updates the block statistics text every frame.
#[allow(dead_code)]
fn update_block_stats_text(
    mut loaded_query: Query<
        &mut Text,
        (
            With<LoadedBlocksText>,
            Without<RenderedBlocksText>,
            Without<CullingEfficiencyText>,
        ),
    >,
    mut rendered_query: Query<
        &mut Text,
        (
            With<RenderedBlocksText>,
            Without<LoadedBlocksText>,
            Without<CullingEfficiencyText>,
        ),
    >,
    mut efficiency_query: Query<
        &mut Text,
        (
            With<CullingEfficiencyText>,
            Without<LoadedBlocksText>,
            Without<RenderedBlocksText>,
        ),
    >,
) {
    // Update loaded blocks text
    for mut text in &mut loaded_query {
        **text = format!("Loaded Blocks: --");
    }

    // Update rendered blocks text
    for mut text in &mut rendered_query {
        **text = format!("Rendered Blocks: --");
    }

    // Update culling efficiency text
    for mut text in &mut efficiency_query {
        **text = "Culling: --%".to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_builds() {
        let mut app = App::new();
        app.add_plugins(DebugUiPlugin);
        // If this doesn't panic, the plugin is valid
    }
}
