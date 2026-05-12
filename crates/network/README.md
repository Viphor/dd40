# dd40_network

Network transport and replication crate for dd40, built on
[lightyear](https://github.com/cBournhonesque/lightyear). Handles the full
client-server lifecycle: connection management, player input forwarding,
authoritative character replication, block-change propagation, and chunk
streaming to newly connected clients.

Depends only on `dd40_core`. Feature-flagged: compile with `client`, `server`,
or both (default).

## Module overview

```
src/
├── lib.rs             — public re-exports: ClientNetworkPlugin, ServerNetworkPlugin, protocol types
├── protocol.rs        — shared protocol: PlayerInput, PlayerPosition, PlayerRotation, PlayerSpeed,
│                        NetworkCharacter, PlayerSpawnLocation,
│                        PlayerJoinedMessage, PlayerLeftMessage, channels, ProtocolPlugin
│
├── shared/
│   ├── mod.rs         — shared module declarations
│   ├── character.rs   — shared character replication helpers
│   └── connection.rs  — SHARED_SETTINGS, SERVER_ADDR, SERVER_PORT, CLIENT_PORT constants
│
├── client/            — (feature: client)
│   ├── mod.rs
│   ├── plugin.rs      — ClientNetworkPlugin
│   ├── connection.rs  — DDClient config, lightyear client setup
│   ├── character.rs   — frame interpolation, visual correction of predicted position
│   ├── chunk_provider.rs — receives chunk data from server and writes ChunkReady messages
│   ├── loading.rs     — loading tracker integration (waits for server connection)
│   └── spawn.rs       — spawns the local player entity on PlayerSpawnLocation receipt
│
└── server/            — (feature: server)
    ├── mod.rs
    ├── plugin.rs      — ServerNetworkPlugin(DDServer)
    ├── connection.rs  — DDServer config, lightyear server setup, LinkConditioner
    ├── character.rs   — replicates character components to clients
    ├── chunk_provider.rs — streams chunk data to clients on request
    ├── chunk_requests.rs — handles client chunk requests and triggers loading
    ├── user.rs        — tracks connected peer state
    └── spawn.rs       — WorldSpawnConfig resource, PlayerLocations resource, spawn-on-connect logic
```

## Known inconsistency

`PlayerLocations` (in `server/spawn.rs`) is keyed by lightyear `PeerId`. This
couples spawn-point management to the network identity system and will need to
be decoupled before NPCs, animals, or alternative spawn providers can be added.
See `INCONSISTENCIES.md`.
