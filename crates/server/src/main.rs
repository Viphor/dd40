use bevy::prelude::*;
use dd40_core::CorePlugin;
use dd40_player::PlayerPlugin;
use dd40_world::WorldPlugin;

fn main() {
    App::new()
        // MinimalPlugins gives us ECS, scheduling, and time – but no window or rendering.
        .add_plugins(MinimalPlugins)
        .add_plugins((CorePlugin, WorldPlugin, PlayerPlugin))
        .add_systems(Update, server_tick)
        .run();
}

/// Placeholder server tick system – extend with network and game-logic code.
fn server_tick(_time: Res<Time>) {}
