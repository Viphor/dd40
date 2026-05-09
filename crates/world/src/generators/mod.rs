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

/// Initial version stamped on every generated chunk.
///
/// World generators output the *first* authoritative state of a chunk;
/// version `0` is reserved for "client has nothing yet, send a snapshot",
/// so the first authoritative state must be at least `1`. The chunk
/// authority bumps the version monotonically from there.
pub const GENERATED_CHUNK_VERSION: u64 = 1;

/// System that listens for [`GenerateChunk`] events,
/// generates the requested chunk using the provided [`WorldGenerator`],
/// and emits a [`ChunkReady`] event with the generated chunk.
///
/// The chunk's version is overridden to [`GENERATED_CHUNK_VERSION`] before
/// emission, so individual [`WorldGenerator`] implementations do not need
/// to think about versioning.
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
        let mut chunk = generator.generate_chunk(event.pos);
        chunk.set_version(GENERATED_CHUNK_VERSION);
        writer.write(ChunkReady { chunk });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::prelude::*;

    #[derive(Resource, Clone)]
    struct DummyGen;

    impl WorldGenerator for DummyGen {
        fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
            // Returns a fresh chunk at version 0 — generate_chunks must override.
            Chunk::new(pos)
        }
    }

    #[test]
    fn generated_chunks_are_stamped_at_version_one() {
        let mut app = App::new();
        app.insert_resource(DummyGen)
            .add_message::<GenerateChunk>()
            .add_message::<ChunkReady>()
            .add_systems(Update, generate_chunks::<DummyGen>);

        app.world_mut()
            .resource_mut::<Messages<GenerateChunk>>()
            .write(GenerateChunk {
                pos: ChunkPos::new(0, 0),
            });

        app.update();

        let messages = app.world().resource::<Messages<ChunkReady>>();
        let mut cursor = messages.get_cursor();
        let ready: Vec<_> = cursor.read(messages).collect();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].chunk.version(), GENERATED_CHUNK_VERSION);
    }
}
