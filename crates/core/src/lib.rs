pub mod block;
pub mod chunk;
pub mod common;
pub mod debug;
pub mod loading;
pub mod macros;
pub mod plugin;
pub mod state;
pub mod tools;

pub mod prelude {
    pub use crate::{
        block::{
            Block, BlockDefinition, BlockId, BlockPos, BlockRegistry, CollisionShape, events::*,
            registry::BlockRegistrySet,
        },
        chunk::{
            CHUNK_SIZE, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk, ChunkPos,
            cache::ChunkCache, events::*,
        },
        loading::{LoadingPlugin, LoadingSet, LoadingTracker},
        state::{AppState, GameState},
        tools::{
            ToolKindDefinition, ToolKindId, ToolRegistry, ToolRegistrySet,
            ToolTierDefinition, ToolTierId, mining_duration,
        },
    };
}
