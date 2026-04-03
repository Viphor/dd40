# dd40_network

Network communication layer for dd40 using the [lightyear](https://github.com/cBournhonesque/lightyear) networking library.

## ⚠️ Current Status: Skeleton Implementation

This crate provides the **foundational structure** for client-server networking but requires completion of the lightyear integration. The protocol definitions, message types, and component types are fully defined and documented, but the actual network transport and replication logic needs to be implemented.

## Overview

This crate is organized into three main modules:

- **`protocol`** - Shared protocol definitions (messages, components, channels, inputs)
- **`client`** - Client-side networking (connection, input sending, state reception)
- **`server`** - Server-side networking (connections, input processing, state replication)

## Architecture

### Protocol (`protocol.rs`)

Defines the shared network protocol used by both client and server:

#### Components (Replicated)
- `PlayerPosition` - Player's world position (Vec3)
- `PlayerRotation` - Player's camera rotation (pitch, yaw)
- `PlayerSpeed` - Player's movement speed

#### Messages
- `RequestChunks` - Client → Server chunk request
- `ChunkData` - Server → Client chunk data
- `BlockPlacedMessage` - Server → Client block placement notification
- `BlockRemovedMessage` - Server → Client block removal notification
- `BlockChangedMessage` - Server → Client block change notification
- `PlayerJoinedMessage` - Server → Client player join notification
- `PlayerLeftMessage` - Server → Client player leave notification

#### Inputs
- `PlayerInput` - Client input sent every frame (movement, rotation, actions)

#### Channels
- `InputChannel` - Unreliable unordered (for high-frequency inputs)
- `BlockChannel` - Reliable ordered (for block changes)
- `ChunkChannel` - Reliable unordered (for chunk data)
- `EventChannel` - Reliable ordered (for game events)

### Client (`client.rs`)

**Status**: Skeleton implementation

The client module defines:
- `ClientNetworkConfig` - Configuration resource
- `ClientNetworkPlugin` - Plugin to add to client apps

**TODO**:
1. Configure lightyear client plugin with proper transport
2. Implement `collect_player_input` system
3. Implement message handling systems
4. Set up entity replication from server
5. Implement client-side prediction

### Server (`server.rs`)

**Status**: Skeleton implementation

The server module defines:
- `ServerNetworkConfig` - Configuration resource
- `ServerNetworkPlugin` - Plugin to add to server apps
- `ServerPlayer` - Component marking server-side player entities

**TODO**:
1. Configure lightyear server plugin with proper transport
2. Implement client connection/disconnection handling
3. Implement input processing and authoritative simulation
4. Set up entity replication to clients
5. Implement observers to broadcast block changes

## Usage

### Adding to Your Project

```toml
[dependencies]
dd40_network = { path = "crates/network" }
```

### Client Setup

```rust
use bevy::prelude::*;
use dd40_network::client::{ClientNetworkPlugin, ClientNetworkConfig};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(ClientNetworkConfig {
            server_addr: "127.0.0.1:5000".parse().unwrap(),
            client_addr: "0.0.0.0:0".parse().unwrap(),
            client_id: rand::random(),
        })
        .add_plugins(ClientNetworkPlugin)
        .run();
}
```

### Server Setup

```rust
use bevy::prelude::*;
use dd40_network::server::{ServerNetworkPlugin, ServerNetworkConfig};

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .insert_resource(ServerNetworkConfig {
            bind_addr: "0.0.0.0:5000".parse().unwrap(),
            max_clients: 16,
        })
        .add_plugins(ServerNetworkPlugin)
        .run();
}
```

## Completing the Implementation

To complete the lightyear integration, follow these steps:

### 1. Study Lightyear Examples

The lightyear API is complex and version-specific. Start by studying the official examples:
- https://github.com/cBournhonesque/lightyear/tree/main/examples/simple_box

### 2. Configure Transport

Choose and configure a transport (UDP, WebTransport, Steam, etc.):

```rust
// Example (actual API may vary)
let net_config = NetConfig::Netcode {
    config: NetcodeConfig::default(),
    io: IoConfig::from_transport(TransportConfig::UdpSocket(bind_addr)),
};
```

### 3. Set Up Client Plugin

In `client.rs`, uncomment and complete the lightyear client plugin setup in the `ClientNetworkPlugin::build` method.

### 4. Set Up Server Plugin

In `server.rs`, uncomment and complete the lightyear server plugin setup in the `ServerNetworkPlugin::build` method.

### 5. Implement Input Collection

In `client.rs`, implement the `collect_player_input` system to:
- Read keyboard/mouse input
- Create `PlayerInput` struct
- Send via lightyear's input system

### 6. Implement Authoritative Simulation

In `server.rs`, implement the `process_player_inputs` system to:
- Read inputs from clients
- Update player positions/rotations authoritatively
- Mark components as changed for replication

### 7. Set Up Replication

Configure component replication in the protocol:
- Mark components with `Replicate` bundle
- Set up prediction/interpolation as needed

### 8. Implement Message Handlers

Complete the message handling systems to:
- Receive messages from network
- Convert to local events
- Trigger appropriate systems

## Testing

Run tests with:

```bash
cargo test --package dd40_network
```

All protocol helper functions and conversions are tested.

## Edge Cases to Consider

When completing the implementation, consider these edge cases:

1. **Connection Loss**: Handle client disconnection gracefully
2. **Late Joiners**: Send full world state to newly connected clients
3. **Input Buffering**: Handle input loss or duplication
4. **Chunk Boundaries**: Ensure blocks near chunk edges replicate correctly
5. **Entity Despawning**: Clean up replicated entities on disconnect
6. **Version Mismatch**: Handle protocol version differences
7. **Bandwidth Limits**: Don't send too much data at once
8. **Clock Sync**: Handle client/server time differences
9. **Invalid Block IDs**: Validate block IDs from network
10. **Concurrent Modifications**: Handle multiple clients modifying same block

## Integration with Other Crates

### dd40_core

The network crate observes these events from core:
- `BlockPlaced`
- `BlockRemoved`
- `BlockChanged`

When fully implemented, these will be broadcast to all connected clients.

### dd40_world

The server should generate chunks on demand when clients request them via `RequestChunks` messages.

### dd40_player

Player movement should be:
- **Client**: Predicted locally, inputs sent to server
- **Server**: Authoritative, simulated from inputs, replicated to clients

## Performance Considerations

- Use unreliable channels for high-frequency data (inputs, positions)
- Use reliable channels for critical data (block changes, events)
- Batch block changes when possible
- Send only visible blocks in chunk data
- Use interpolation for smooth remote player movement
- Use prediction for local player to hide latency

## Documentation

All public items are documented with rustdoc comments. Generate documentation with:

```bash
cargo doc --package dd40_network --open
```

## License

Same as the parent dd40 project.