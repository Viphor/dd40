use bevy::{prelude::*, state::app::StatesPlugin};

use crate::{chunk::cache::ChunkCachePlugin, prelude::*, vanilla_blocks::setup_vanilla_blocks};

/// Bevy plugin that registers core types with the reflection system.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }

        app.init_state::<AppState>()
            .init_state::<GameState>()
            .insert_resource(BlockRegistry::new())
            .register_type::<BlockId>()
            .register_type::<Block>()
            .register_type::<BlockRegistry>()
            .register_type::<ChunkPos>()
            .register_type::<BlockPos>()
            .add_message::<ChunkReady>()
            .add_message::<RequestChunk>()
            .add_message::<GenerateChunk>()
            .add_message::<BlockPlaced>()
            .add_message::<BlockRemoved>()
            .add_message::<BlockChanged>()
            .configure_sets(
                Startup,
                (BlockRegistrySet, WorldGenerationSet.after(BlockRegistrySet)),
            )
            .add_systems(Startup, setup_vanilla_blocks.in_set(BlockRegistrySet));

        app.add_plugins(ChunkCachePlugin);
    }
}
