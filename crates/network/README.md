# dd40_network

Network transport and replication crate for dd40, built on
[lightyear](https://github.com/cBournhonesque/lightyear). Handles the full
client-server lifecycle: connection management, authoritative character
replication via `bevy_enhanced_input`, block-change propagation, and
chunk streaming to newly connected clients.

This crate is **input-stack agnostic** — it spawns characters with the
`OnFoot` context marker only. The actual `Action<T>` entities + bindings
are created by `dd40_player_input::spawn_local_player_input_tree` on the
client; the server registers action *types* via lightyear's
`InputRegistryExt::add_input::<OnFoot>` so values replicate.

Depends only on `dd40_core` and `dd40_input_core`. Feature-flagged:
compile with `client`, `server`, or both (default).

## Module overview

```
src/
├── lib.rs                — public re-exports + ClientNetworkPlugin / ServerNetworkPlugin
├── protocol.rs           — shared protocol: PlayerPosition, PlayerRotation, PlayerSpeed,
│                           NetworkCharacter, PlayerSpawnLocation, channels, ProtocolPlugin
├── character_ext.rs      — client/server EntityCommands traits that attach OnFoot,
│                           InputMarker<OnFoot>, Player (client) or Character (server)
│
├── shared/
│   ├── mod.rs
│   └── connection.rs     — SHARED_SETTINGS, SERVER_ADDR, SERVER_PORT, CLIENT_PORT
│
├── client/               — (feature: client)
│   ├── plugin.rs         — ClientNetworkPlugin
│   ├── connection.rs     — DDClient config, lightyear client setup
│   ├── character.rs      — frame interpolation; bridge_camera_rotation_to_action
│   │                       (CharacterInput.{yaw,pitch} → Action<CameraRotation>)
│   ├── chunk_provider.rs — receives chunk data from server, writes ChunkReady messages
│   ├── loading.rs        — loading tracker integration
│   └── spawn.rs          — spawns the local player entity on PlayerSpawnLocation receipt
│
└── server/               — (feature: server)
    ├── plugin.rs         — ServerNetworkPlugin(DDServer)
    ├── connection.rs     — DDServer config, LinkConditioner
    ├── character.rs      — replicates character components
    ├── chunk_provider.rs — streams chunk data to clients on request
    ├── chunk_requests.rs — handles client chunk requests
    ├── user.rs           — tracks connected peer state
    └── spawn.rs          — WorldSpawnConfig, PlayerLocations, spawn-on-connect logic
```

## Known inconsistency

`PlayerLocations` (in `server/spawn.rs`) is keyed by lightyear `PeerId`. This
couples spawn-point management to the network identity system and will need to
be decoupled before NPCs, animals, or alternative spawn providers can be added.
See `INCONSISTENCIES.md`.
