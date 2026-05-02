use bevy::{prelude::*, state::app::StatesPlugin};

use crate::{
    chunk::cache::ChunkCachePlugin,
    loading::LoadingPlugin,
    prelude::*,
    tools::{ToolRegistry, configure_tool_registry_ordering},
};

/// Bevy plugin that registers core types with the reflection system.
#[derive(Default)]
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }

        app.add_plugins(LoadingPlugin)
            .init_state::<AppState>()
            .init_state::<GameState>()
            .insert_resource(BlockRegistry::new())
            .insert_resource(ToolRegistry::new())
            .register_type::<BlockId>()
            .register_type::<Block>()
            .register_type::<BlockRegistry>()
            .register_type::<CollisionShape>()
            .register_type::<ToolRegistry>()
            .register_type::<ChunkPos>()
            .register_type::<BlockPos>()
            .add_message::<ChunkReady>()
            .add_message::<RequestChunk>()
            .add_message::<GenerateChunk>()
            .add_message::<BlockPlaced>()
            .add_message::<BlockRemoved>()
            .add_message::<BlockChanged>()
            .add_message::<StartMiningRequest>()
            .add_message::<AbortMiningRequest>()
            .add_message::<MineBlockRequest>();

        // System set ordering: tools must be registered before blocks (block
        // definitions may reference ToolKindId), and both must finish before
        // world generation reads the registry.
        configure_tool_registry_ordering(app);
        app.configure_sets(
            Startup,
            (BlockRegistrySet, WorldGenerationSet.after(BlockRegistrySet)),
        );

        app.add_plugins(ChunkCachePlugin);
    }
}
