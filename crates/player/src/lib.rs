use bevy::prelude::*;

/// Marker component that identifies the player entity.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct Player;

/// Walking / flying speed of the player in units per second.
#[derive(Debug, Component, Reflect)]
#[reflect(Component)]
pub struct MovementSpeed(pub f32);

impl Default for MovementSpeed {
    fn default() -> Self {
        Self(5.0)
    }
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Player,
        MovementSpeed::default(),
        Transform::from_xyz(0.0, 64.0, 0.0),
        GlobalTransform::default(),
    ));
}

/// Bevy plugin that registers player types and spawns the player on startup.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Player>()
            .register_type::<MovementSpeed>()
            .add_systems(Startup, spawn_player);
    }
}
