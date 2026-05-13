//! Network protocol definitions shared between client and server.
//!
//! This module defines all the message types and component types that can be
//! replicated over the network using lightyear. The protocol is registered as
//! a plugin and should be added to both client and server apps.

use bevy::math::Curve;
use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_core::prelude::*;
use dd40_physics_core::prelude::Velocity;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

// ============================================================================
// CHANNELS
// ============================================================================

/// Unreliable unordered channel for player inputs (high frequency, loss tolerant)
pub struct InputChannel;

/// Reliable ordered channel for block changes (must arrive in order)
pub struct BlockChannel;

/// Reliable unordered channel for chunk data (large, must arrive but order doesn't matter)
pub struct ChunkChannel;

/// Reliable ordered channel for important game events
pub struct EventChannel;

// ============================================================================
// INPUTS
// ============================================================================

/// Client input that is sent every fixed tick.
///
/// The server applies these inputs authoritatively to the character entity.
/// The controlling client mirrors the same logic on its [`Predicted`] entity
/// for client-side prediction.
///
/// The action triple ([`attack`](Self::attack) / [`interact`](Self::interact)
/// / [`place`](Self::place)) is intentionally split: keeping policy ("does
/// right-click interact or place?") out of the protocol lets the local-player
/// input layer decide per right-click while the interaction/placement systems
/// stay agnostic.
///
/// [`Predicted`]: lightyear::prelude::client::Predicted
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PlayerInput {
    /// Movement direction in world space (normalised or zero).
    pub movement: Vec3,
    /// Camera pitch (up/down rotation in radians).
    pub pitch: f32,
    /// Camera yaw (left/right rotation in radians).
    pub yaw: f32,
    /// Whether the player wants to jump this tick.
    pub jump: bool,
    /// Whether the player is sprinting (doubles [`MovementSpeed`]).
    ///
    /// [`MovementSpeed`]: dd40_core::character::MovementSpeed
    pub sprint: bool,
    /// Continuous primary-action intent. Held while the player wants to
    /// mine (and, eventually, melee-attack).
    pub attack: bool,
    /// One-shot secondary-action intent — interact with the targeted
    /// block (lever, button, container).
    pub interact: bool,
    /// One-shot intent to place a block from the player's active item.
    pub place: bool,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            movement: Vec3::ZERO,
            pitch: 0.0,
            yaw: 0.0,
            jump: false,
            sprint: false,
            attack: false,
            interact: false,
            place: false,
        }
    }
}

impl bevy::ecs::entity::MapEntities for PlayerInput {
    fn map_entities<M: bevy::ecs::entity::EntityMapper>(&mut self, _mapper: &mut M) {
        // PlayerInput contains no entity references.
    }
}

// ============================================================================
// MESSAGES
// ============================================================================

// Request chunks around a center position
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct RequestChunks {
//     /// Center position (usually player position)
//     pub center: BlockPos,
//     /// Radius in chunks (e.g., radius 3 = 7x7 chunk area)
//     pub radius: u32,
// }

/// Request a spawn of the player with the given client id. The server responds with a [`PlayerSpawnLocation`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestSpawn(pub u64);

/// Sent by the server to a client immediately after it connects.
///
/// The server resolves the player's last known position (keyed by their
/// lightyear peer id) or falls back to the world's configured default spawn,
/// then streams this message followed by the 9 surrounding [`RequestChunk`]
/// responses through the normal chunk pipeline.
///
/// The client uses this position to seed [`InitialChunksGate`] with the
/// expected 3×3 chunk grid and to place the player entity once all chunks
/// have arrived.
///
/// [`InitialChunksGate`]: crate::client::InitialChunksGate
#[derive(Message, Clone, Serialize, Deserialize, Debug)]
pub struct PlayerSpawnLocation {
    /// World-space position the player should spawn at.
    pub position: Vec3,
}

/// Message sent when a player joins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJoinedMessage {
    pub player_name: String,
}

/// Message sent when a player leaves
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerLeftMessage {
    pub player_name: String,
}

/// Server-broadcast delta of authoritatively-committed changes to a chunk.
///
/// Sent from the server to every client that already has the chunk loaded,
/// once per [`ChunkChanged`] emission from the chunk-authority commit pass.
///
/// The client applies the delta only if `base_version == local_version`.
/// If `base_version > local_version` the client is ahead of the server
/// (impossible in a healthy session — log + drop). If `base_version <
/// local_version` the client is behind: it re-issues a [`RequestChunk`]
/// with its current version so the server can reply with either a
/// catch-up [`ChunkUpdate`] or a [`ChunkSnapshot`].
///
/// [`ChunkChanged`]: dd40_core::chunk::events::ChunkChanged
/// [`RequestChunk`]: dd40_core::chunk::events::RequestChunk
#[derive(Message, Clone, Debug, Serialize, Deserialize)]
pub struct ChunkUpdate {
    /// Chunk the delta targets.
    pub pos: ChunkPos,
    /// Version the chunk was at *before* `changes` were applied. The
    /// client may apply the delta only if this matches its local version.
    pub base_version: u64,
    /// Authoritative changes, in the order the server applied them.
    pub changes: Vec<ChunkChange>,
    /// Version after `changes` are applied. Clients store this as their
    /// new local version on success.
    pub new_version: u64,
}

/// Server-sent full snapshot of a chunk.
///
/// Sent in response to a [`RequestChunk`] when the server cannot satisfy
/// the request with a [`ChunkUpdate`] — typically because:
///
/// - The client requested with `current_version == 0` (no local copy).
/// - The client is more than `MaxDeltaBehind` versions behind.
/// - The chunk's confirmed history has been truncated below the client's
///   version.
/// - The client requested with a version newer than the server's (a bug
///   on the client; the snapshot is sent anyway to recover).
///
/// Carries a fully-formed [`Chunk`] including its current version. The
/// client inserts it via the normal [`ChunkReady`] pipeline.
///
/// [`RequestChunk`]: dd40_core::chunk::events::RequestChunk
/// [`ChunkReady`]: dd40_core::chunk::events::ChunkReady
#[derive(Message, Clone, Debug, Serialize, Deserialize)]
pub struct ChunkSnapshot {
    /// The chunk, at its current authoritative version.
    pub chunk: Chunk,
}

// ============================================================================
// MARKER COMPONENTS
// ============================================================================

/// Marker component replicated to all clients so they can identify networked
/// character entities.
///
/// Registering this as a replicated lightyear component means it is present
/// on the server entity **and** on the client-side [`Predicted`] /
/// [`Interpolated`] copies.  Client systems filter on `With<NetworkCharacter>`
/// to locate the right entities without depending on any other crate.
///
/// [`Predicted`]: lightyear::prelude::client::Predicted
/// [`Interpolated`]: lightyear::prelude::client::Interpolated
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default, Reflect)]
#[reflect(Component)]
pub struct NetworkCharacter;

/// Replicated player position
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct PlayerPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl PlayerPosition {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn from_vec3(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

impl Ease for PlayerPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| PlayerPosition {
            x: start.x + (end.x - start.x) * t,
            y: start.y + (end.y - start.y) * t,
            z: start.z + (end.z - start.z) * t,
        })
    }
}

/// Replicated player rotation
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct PlayerRotation {
    pub pitch: f32,
    pub yaw: f32,
}

impl PlayerRotation {
    pub fn new(pitch: f32, yaw: f32) -> Self {
        Self { pitch, yaw }
    }
}

impl Ease for PlayerRotation {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| PlayerRotation {
            pitch: start.pitch + (end.pitch - start.pitch) * t,
            yaw: start.yaw + (end.yaw - start.yaw) * t,
        })
    }
}

/// Replicated player speed
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Reflect)]
pub struct PlayerSpeed {
    pub speed: f32,
}

impl PlayerSpeed {
    pub fn new(speed: f32) -> Self {
        Self { speed }
    }
}

impl Default for PlayerSpeed {
    fn default() -> Self {
        Self { speed: 5.0 }
    }
}

// ============================================================================
// PROTOCOL PLUGIN
// ============================================================================

/// Plugin that registers the network protocol.
///
/// This should be added to both client and server apps to ensure they use
/// the same protocol definitions.
#[derive(Clone)]
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Register reflection types
        app.register_type::<PlayerInput>()
            .register_type::<PlayerPosition>()
            .register_type::<PlayerRotation>()
            .register_type::<PlayerSpeed>()
            .register_type::<NetworkCharacter>();

        // Register the native input plugin so PlayerInput is tick-synced
        // between client and server via lightyear's input pipeline.
        app.add_plugins(lightyear::prelude::input::native::InputPlugin::<PlayerInput>::default());

        // Register components with replication.
        //
        // `NetworkCharacter` and `Character` are markers that never change,
        // so `replicate_once` sends them once at spawn instead of every tick.
        // Neither needs prediction: the markers are present on the Confirmed
        // entity and the client-side predicted-entity systems filter on
        // `With<Predicted>` (the only Predicted entity is the local
        // character) rather than on the markers.
        app.register_component::<NetworkCharacter>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..Default::default()
            });
        app.register_component::<Character>()
            .with_replication_config(ComponentReplicationConfig {
                replicate_once: true,
                ..Default::default()
            });

        app.register_component::<PlayerPosition>()
            .add_prediction()
            .add_linear_interpolation();

        // Velocity must be predicted alongside position so rollback re-simulation
        // starts from the correct (position, velocity) pair.  Without this,
        // restoring position but not velocity causes the re-simulation to
        // immediately diverge, producing visible drift especially during jumps
        // and gravity-driven falls.
        app.register_component::<Velocity>().add_prediction();

        // Rotation is NOT predicted — the controlling client writes it locally
        // each PostUpdate frame from the camera, so it is always perfectly
        // smooth.  Other clients receive it with linear interpolation.
        app.register_component::<PlayerRotation>()
            .add_linear_interpolation();

        app.register_component::<PlayerSpeed>();

        // Register messages with directions
        // Client -> Server
        app.register_message::<RequestChunk>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<RequestSpawn>()
            .add_direction(NetworkDirection::ClientToServer);

        // Server -> Client
        app.register_message::<PlayerSpawnLocation>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<ChunkUpdate>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<ChunkSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<PlayerJoinedMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<PlayerLeftMessage>()
            .add_direction(NetworkDirection::ServerToClient);

        // Register channels
        app.add_channel::<InputChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);

        app.add_channel::<BlockChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<ChunkChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<EventChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Helper function to convert global block position to chunk position and local position.
pub fn global_to_chunk_local(global_pos: &BlockPos) -> (ChunkPos, (u8, u8, u8)) {
    let chunk_x = global_pos.x.div_euclid(16);
    let chunk_z = global_pos.z.div_euclid(16);

    let local_x = global_pos.x.rem_euclid(16) as u8;
    let local_y = global_pos.y as u8; // Assuming y is always 0-255
    let local_z = global_pos.z.rem_euclid(16) as u8;

    (
        ChunkPos::new(chunk_x, 0, chunk_z),
        (local_x, local_y, local_z),
    )
}

/// Helper function to convert chunk position and local position to global block position.
pub fn chunk_local_to_global(chunk_pos: &ChunkPos, local_pos: (u8, u8, u8)) -> BlockPos {
    let global_x = chunk_pos.x * 16 + local_pos.0 as i32;
    let global_y = local_pos.1 as i32;
    let global_z = chunk_pos.z * 16 + local_pos.2 as i32;

    BlockPos::new(global_x, global_y, global_z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_to_chunk_local() {
        let pos = BlockPos::new(17, 64, -5);
        let (chunk_pos, local_pos) = global_to_chunk_local(&pos);

        assert_eq!(chunk_pos.x, 1);
        assert_eq!(chunk_pos.z, -1);
        assert_eq!(local_pos, (1, 64, 11));
    }

    #[test]
    fn test_chunk_local_to_global() {
        let chunk_pos = ChunkPos::new(2, 0, -3);
        let local_pos = (5, 100, 7);
        let global_pos = chunk_local_to_global(&chunk_pos, local_pos);

        assert_eq!(global_pos.x, 37);
        assert_eq!(global_pos.y, 100);
        assert_eq!(global_pos.z, -41);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = BlockPos::new(123, 45, -67);
        let (chunk_pos, local_pos) = global_to_chunk_local(&original);
        let result = chunk_local_to_global(&chunk_pos, local_pos);

        assert_eq!(original.x, result.x);
        assert_eq!(original.y, result.y);
        assert_eq!(original.z, result.z);
    }

    #[test]
    fn test_player_position_conversions() {
        let vec = Vec3::new(1.5, 2.5, 3.5);
        let pos = PlayerPosition::from_vec3(vec);
        assert_eq!(pos.to_vec3(), vec);
    }

    #[test]
    fn test_player_input_default() {
        let input = PlayerInput::default();
        assert_eq!(input.movement, Vec3::ZERO);
        assert_eq!(input.pitch, 0.0);
        assert_eq!(input.yaw, 0.0);
        assert!(!input.jump);
        assert!(!input.sprint);
        assert!(!input.attack);
        assert!(!input.interact);
        assert!(!input.place);
    }
}
