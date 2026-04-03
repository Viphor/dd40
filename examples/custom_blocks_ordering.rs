//! Example demonstrating how to use SystemSets to ensure block registration
//! happens before world generation.
//!
//! This example shows:
//! - Registering custom blocks in the BlockRegistrySet
//! - Running world generation in the WorldGenerationSet
//! - The automatic ordering ensures blocks are registered before world gen runs

use bevy::prelude::*;
use dd40_core::{
    BlockDefinition, BlockId, BlockPos, BlockRegistry, BlockRegistrySet, WorldGenerationSet,
};
use dd40_world::spawn_block;

// Define custom block IDs (use IDs >= 1000 to avoid conflicts with vanilla blocks)
pub const COPPER_ORE: BlockId = BlockId(1000);
pub const EMERALD_ORE: BlockId = BlockId(1001);
pub const RUBY_ORE: BlockId = BlockId(1002);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Custom Blocks with Ordering Example".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            dd40_core::CorePlugin,
            dd40_world::ClientWorldPlugin,
            dd40_world::ServerWorldPlugin,
            CustomBlocksPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

/// Plugin that registers custom blocks and spawns them in the world.
pub struct CustomBlocksPlugin;

impl Plugin for CustomBlocksPlugin {
    fn build(&self, app: &mut App) {
        // Register custom blocks in BlockRegistrySet
        app.add_systems(Startup, register_custom_blocks.in_set(BlockRegistrySet));

        // Spawn custom blocks in WorldGenerationSet (runs after BlockRegistrySet)
        app.add_systems(Startup, spawn_custom_ore_line.in_set(WorldGenerationSet));
    }
}

/// Registers custom ore blocks into the BlockRegistry.
/// This system runs in BlockRegistrySet, ensuring it completes before world generation.
fn register_custom_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    info!("Registering custom ore blocks...");

    registry.register(
        BlockDefinition::new(COPPER_ORE, "copper_ore")
            .with_color(Color::srgb(0.8, 0.5, 0.3))
            .with_solid(true)
            .with_renderable(true),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(EMERALD_ORE, "emerald_ore")
            .with_color(Color::srgb(0.2, 0.9, 0.4))
            .with_solid(true)
            .with_renderable(true),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(RUBY_ORE, "ruby_ore")
            .with_color(Color::srgb(0.9, 0.1, 0.2))
            .with_solid(true)
            .with_renderable(true),
        &mut commands,
    );

    info!("Custom ore blocks registered successfully!");
}

/// Spawns a line of custom ore blocks in the world.
/// This system runs in WorldGenerationSet, guaranteeing that all block
/// registrations (including from other plugins) have completed.
fn spawn_custom_ore_line(mut commands: Commands, registry: Res<BlockRegistry>) {
    info!("Spawning custom ore line...");

    // Verify blocks are registered before spawning
    if registry.get(COPPER_ORE).is_none() {
        error!("COPPER_ORE not registered! This shouldn't happen with proper ordering.");
        return;
    }

    // Spawn a line of different ores at y=70 for visibility
    let y = 70;
    for x in -5..=5 {
        let block_id = match x {
            -5..=-2 => COPPER_ORE,
            -1..=1 => EMERALD_ORE,
            2..=5 => RUBY_ORE,
            _ => continue,
        };

        spawn_block(&mut commands, block_id, BlockPos::new(x, y, 0));
    }

    info!("Custom ore line spawned at y={}", y);
}

/// Sets up the camera and lighting.
fn setup(mut commands: Commands, mut ambient: ResMut<GlobalAmbientLight>) {
    // Spawn camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.0, 75.0, 20.0).looking_at(Vec3::new(0.0, 70.0, 0.0), Vec3::Y),
    ));

    // Set ambient lighting
    ambient.brightness = 1000.0;

    info!("Camera and lighting configured");
}
