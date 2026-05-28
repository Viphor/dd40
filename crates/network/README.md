# dd40_network

Network transport and replication crate for dd40, built on
[lightyear](https://github.com/cBournhonesque/lightyear). Handles the full
client-server lifecycle: connection management, authoritative character
replication, block-change propagation, and chunk streaming to newly
connected clients.

## Wire input format

The networked input is the legacy `ActionState<PlayerInput>` carried via
lightyear's `input_native` feature. `PlayerInput` mirrors `CharacterInput`
(movement, jump, sprint, attack, place, interact, yaw, pitch).

The client-side BEI pipeline lives in `dd40_player_input` and never
touches the wire. The handoff happens in
`client::character::bridge_input_to_action_state`:

```
CharacterInput  →  ActionState<PlayerInput>  →  lightyear → server
```

The bridge runs in `FixedPreUpdate` inside `InputSystems::WriteClientInputs`,
**after** `dd40_input_core::system_sets::InputTranslationSet` so it sees
the current tick's translator output (otherwise it would ship the
previous tick's stale `CharacterInput`).

On the server, `server::character::server_apply_inputs` reads the
replicated `ActionState<PlayerInput>` and feeds the shared
`apply_input_to_controller(&ActionState<PlayerInput>, &mut CharacterInput)`
to drive physics. The server does **not** load BEI.

Feature-flagged: compile with `client`, `server`, or both (default).

## Module overview

```
src/
├── lib.rs                — public re-exports + ClientNetworkPlugin / ServerNetworkPlugin
├── protocol.rs           — shared protocol: PlayerPosition, PlayerRotation, PlayerSpeed,
│                           NetworkCharacter, PlayerSpawnLocation, PlayerInput,
│                           channels, ProtocolPlugin
├── character_ext.rs      — client/server EntityCommands traits that attach
│                           Player (client) / Character (server) +
│                           InputMarker<PlayerInput> on the local player
│
├── shared/
│   ├── mod.rs
│   ├── character.rs      — apply_input_to_controller (single source of truth
│   │                       used by both server_apply_inputs and client_apply_inputs)
│   └── connection.rs     — SHARED_SETTINGS, SERVER_ADDR, SERVER_PORT, CLIENT_PORT
│
├── client/               — (feature: client)
│   ├── plugin.rs         — ClientNetworkPlugin
│   ├── connection.rs     — DDClient config, lightyear client setup
│   ├── character.rs      — bridge_input_to_action_state (CharacterInput →
│   │                       ActionState<PlayerInput>),
│   │                       client_apply_inputs (no-op on live ticks;
│   │                       restores CI from ActionState during rollback),
│   │                       frame interpolation, sync_local_rotation
│   ├── chunk_provider.rs — receives chunk data from server, writes ChunkReady messages
│   ├── loading.rs        — loading tracker integration
│   └── spawn.rs          — spawns the local player entity on PlayerSpawnLocation receipt
│
└── server/               — (feature: server)
    ├── plugin.rs         — ServerNetworkPlugin(DDServer)
    ├── connection.rs     — DDServer config, LinkConditioner
    ├── character.rs      — server_apply_inputs (ActionState<PlayerInput> → CharacterInput),
    │                       replicates character components
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
