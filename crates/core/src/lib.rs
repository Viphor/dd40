pub mod block;
pub mod character;
pub mod chunk;
pub mod common;
pub mod debug;
pub mod loading;
pub mod plugin;
pub mod state;
pub mod vanilla_blocks;
pub mod world;

pub mod prelude {
    pub use crate::{
        block::{
            Block, BlockDefinition, BlockId, BlockPos, BlockRegistry, events::*,
            registry::BlockRegistrySet,
        },
        character::physics::{
            Aabb, CharacterCollider, CharacterSpatialCache, CollisionShape, GravityScale, Grounded,
            PhysicsBody, PhysicsConfig, PhysicsPlugin, PhysicsSet, Velocity,
        },
        chunk::{CHUNK_SIZE, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, Chunk, ChunkPos, events::*},
        loading::{LoadingPlugin, LoadingSet, LoadingTracker},
        state::{AppState, GameState},
        world::WorldGenerationSet,
    };
}
