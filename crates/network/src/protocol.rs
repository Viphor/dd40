//! Network protocol definitions shared between client and server.
//!
//! This module defines all the message types and component types that can be
//! replicated over the network using lightyear. The protocol is registered as
//! a plugin and should be added to both client and server apps.

use bevy::math::Curve;
use bevy::prelude::*;
pub use dd40_core::prelude::PlaceBlockRequest;
use dd40_core::prelude::*;
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

/// Client input that is sent every frame.
/// The server will simulate the player based on these inputs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PlayerInput {
    /// Movement direction (normalized or zero)
    pub movement: Vec3,
    /// Camera pitch (up/down rotation)
    pub pitch: f32,
    /// Camera yaw (left/right rotation)
    pub yaw: f32,
    /// Whether the player wants to place a block
    pub place_block: bool,
    /// Whether the player wants to remove a block
    pub remove_block: bool,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            movement: Vec3::ZERO,
            pitch: 0.0,
            yaw: 0.0,
            place_block: false,
            remove_block: false,
        }
    }
}

// ============================================================================
// MESSAGES
// ============================================================================

/// Request chunks around a center position
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

// ============================================================================
// COMPONENTS
// ============================================================================

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
            .register_type::<PlayerSpeed>();

        // Register inputs
        // TODO: Add InputPlugin once lightyear API is properly configured
        // app.add_plugins(InputPlugin::<PlayerInput>::default());

        // Register components with replication
        app.register_component::<PlayerPosition>()
            .add_prediction()
            .add_linear_interpolation();

        app.register_component::<PlayerRotation>()
            .add_prediction()
            .add_linear_interpolation();

        app.register_component::<PlayerSpeed>();

        // Register messages with directions
        // Client -> Server
        app.register_message::<RequestChunk>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<PlaceBlockRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.register_message::<RequestSpawn>()
            .add_direction(NetworkDirection::ClientToServer);

        // Server -> Client
        app.register_message::<PlayerSpawnLocation>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<ChunkReady>()
            .add_direction(NetworkDirection::ServerToClient);

        app.register_message::<BlockPlaced>()
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

    (ChunkPos::new(chunk_x, chunk_z), (local_x, local_y, local_z))
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
        let chunk_pos = ChunkPos::new(2, -3);
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
        assert!(!input.place_block);
        assert!(!input.remove_block);
    }
}
