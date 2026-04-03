use bevy::ecs::{
    message::{MessageReader, MessageWriter},
    resource::Resource,
    system::Res,
};
use dd40_core::prelude::*;

pub mod flat;

pub trait WorldGenerator: Send + Sync + 'static {
    /// Generates a chunk at the given position.
    fn generate_chunk(&self, pos: ChunkPos) -> Chunk;
}

/// System that listens for [`GenerateChunk`] events,
/// generates the requested chunk using the provided [`WorldGenerator`],
/// and emits a [`ChunkReady`] event with the generated chunk.
///
/// NOTE: This expects the generator to be deterministic, so that the same chunk position will always produce the same chunk.
/// This allows the generator to be used in a multi-threaded context without issues.
/// Also the generator should be fast enough to run on the main thread, since this system runs synchronously with chunk requests.
pub(crate) fn generate_chunks<G: WorldGenerator + Resource>(
    generator: Res<G>,
    mut requests: MessageReader<GenerateChunk>,
    mut writer: MessageWriter<ChunkReady>,
) {
    for event in requests.read() {
        let chunk = generator.generate_chunk(event.pos);
        writer.write(ChunkReady { chunk });
    }
}
