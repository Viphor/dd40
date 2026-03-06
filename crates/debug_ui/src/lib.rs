use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

/// Plugin that provides debug UI elements.
/// Currently displays an FPS counter in the top-left corner.
pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .add_systems(Startup, setup_debug_ui)
            .add_systems(Update, update_fps_text);
    }
}

/// Marker component for the FPS counter text.
#[derive(Component)]
struct FpsText;

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
