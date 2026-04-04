//! Example demonstrating the block event system and network forwarding.
//!
//! This example shows:
//! - How to fire BlockPlaced, BlockRemoved, and BlockChanged events
//! - How the NetworkPlugin automatically listens to these events
//! - How events are forwarded to the network layer (simulated with logging)
//!
//! Run this example and watch the console output to see events being captured
//! by the network layer.

use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;
use dd40_core::prelude::{BlockChanged, BlockPlaced, BlockPos, BlockRemoved};
use dd40_core::vanilla_blocks::VanillaBlocks;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(CorePlugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                place_blocks_on_keypress,
                remove_blocks_on_keypress,
                change_blocks_on_keypress,
            ),
        )
        .run();
}

fn setup() {
    info!("=== Block Events Example ===");
    info!("This example demonstrates the event system.");
    info!("Press SPACE to simulate placing blocks");
    info!("Press R to simulate removing blocks");
    info!("Press C to simulate changing blocks");
    info!("Press ESC to exit");
    info!("Watch the console to see network forwarding in action!\n");
}

/// Simulates placing blocks when SPACE is pressed.
/// The NetworkPlugin will automatically pick up these events and "broadcast" them.
fn place_blocks_on_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    mut block_placed_events: MessageWriter<BlockPlaced>,
    time: Res<Time>,
) {
    if keys.just_pressed(KeyCode::Space) {
        let x = (time.elapsed_secs() * 10.0) as i32 % 100;
        let y = 64;
        let z = (time.elapsed_secs() * 5.0) as i32 % 100;

        info!("\n[GAME LOGIC] Player placed a stone block!");

        block_placed_events.write(BlockPlaced {
            pos: BlockPos::new(x, y, z),
            block_id: VanillaBlocks::STONE,
            placer: None, // In a real game, this would be the player entity
        });
    }
}

/// Simulates removing blocks when R is pressed.
fn remove_blocks_on_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    mut block_removed_events: MessageWriter<BlockRemoved>,
    time: Res<Time>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        let x = (time.elapsed_secs() * 7.0) as i32 % 100;
        let y = 65;
        let z = (time.elapsed_secs() * 3.0) as i32 % 100;

        info!("\n[GAME LOGIC] Player removed a block!");

        block_removed_events.write(BlockRemoved {
            pos: BlockPos::new(x, y, z),
            previous_block_id: VanillaBlocks::DIRT,
            remover: None,
        });
    }
}

/// Simulates block transformations when C is pressed.
/// For example, water freezing to ice, or grass dying to dirt.
fn change_blocks_on_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    mut block_changed_events: MessageWriter<BlockChanged>,
    time: Res<Time>,
) {
    if keys.just_pressed(KeyCode::KeyC) {
        let x = (time.elapsed_secs() * 13.0) as i32 % 100;
        let y = 66;
        let z = (time.elapsed_secs() * 11.0) as i32 % 100;

        info!("\n[GAME LOGIC] Grass withered to dirt!");

        block_changed_events.write(BlockChanged {
            pos: BlockPos::new(x, y, z),
            old_block_id: VanillaBlocks::GRASS,
            new_block_id: VanillaBlocks::DIRT,
        });
    }
}
