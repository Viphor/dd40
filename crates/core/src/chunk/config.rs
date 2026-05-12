//! Tunable knobs for the versioned chunk cache.
//!
//! Currently exposes one knob: [`MaxDeltaBehind`]. More may be added as the
//! pipeline grows, but the rule of thumb is "one resource per knob, default
//! sensible, override in the binary if needed".

use bevy::{ecs::resource::Resource, prelude::ReflectResource, reflect::Reflect};

/// How far behind the server's authoritative version a client's
/// [`RequestChunk`](crate::chunk::events::RequestChunk) may be before the
/// server replies with a full snapshot instead of a delta.
///
/// - If `client_version + MaxDeltaBehind >= server_version`, the server
///   sends a `ChunkUpdate` with `history_since(client_version)`.
/// - Otherwise the client is too far behind (or the chunk's history has
///   been truncated by eviction); the server sends a `ChunkSnapshot`.
///
/// The default is `15` — small enough to keep snapshot bandwidth bounded
/// for slowly-reconnecting clients, large enough that normal play almost
/// always uses deltas.
#[derive(Debug, Clone, Copy, Resource, Reflect)]
#[reflect(Resource)]
pub struct MaxDeltaBehind(pub u16);

impl Default for MaxDeltaBehind {
    fn default() -> Self {
        Self(15)
    }
}
