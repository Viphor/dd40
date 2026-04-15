---
name: lightyear
description: "Expert knowledge of the lightyear networking library for Bevy — protocol definition, client/server setup, entity replication, client-side prediction, snapshot interpolation, input handling, channels, and messaging. Invoke this skill for ANY lightyear or multiplayer Bevy question: writing/debugging lightyear code; setting up ClientPlugins/ServerPlugins; defining a ProtocolPlugin; registering components, messages, or channels; replicating entities; enabling prediction or interpolation; handling networked player inputs; sending messages between server and clients; connection lifecycle; entity mapping (MapEntities) for replicated components; questions about PredictionTarget, InterpolationTarget, Replicate, ReplicationSender, or ActionState. Also trigger when the user asks how to add multiplayer to a Bevy game, sync state between clients, or reports lightyear errors. Do not trigger for general Bevy questions that have nothing to do with networking."
---

# Lightyear Expert Reference

**Version:** lightyear 0.26 → Bevy 0.18  
**Docs:** https://cbournhonesque.github.io/lightyear/book/  
**API:** https://docs.rs/lightyear  
**Examples:** https://github.com/cBournhonesque/lightyear/tree/main/examples

| Lightyear | Bevy |
|-----------|------|
| 0.26      | 0.18 |
| 0.25      | 0.17 |
| 0.20–0.24 | 0.16 |

---

## Architecture Overview

Lightyear is a **server-authoritative** networking library. The server owns the truth; clients predict or interpolate to stay responsive. Everything flows through a **layered stack**:

```
IO (UDP/WebTransport/WebSocket/Steam)
  └── Connection (Netcode/Steam — gives peers a stable PeerId)
        └── Messages (channels with reliability/ordering)
              └── Replication (syncs ECS state)
                    └── Prediction / Interpolation (client smoothing)
```

### Code organisation (recommended)

| File | Contents |
|------|----------|
| `protocol.rs` | Shared: components, messages, inputs, channels |
| `shared.rs` | Shared simulation logic (movement, physics) |
| `server.rs` | Server plugin: spawning, replication setup, input processing |
| `client.rs` | Client plugin: input buffering, prediction systems, message receiving |
| `main.rs` | App construction, selecting client vs. server mode |

---

## ⚠️ Critical: Plugin Ordering

**Add `ClientPlugins`/`ServerPlugins` BEFORE your `ProtocolPlugin`.** Swapping the order causes a runtime panic because the registries (ComponentRegistry, MessageRegistry, etc.) don't exist yet.

```rust
fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    
    // 1. Lightyear infrastructure first
    app.add_plugins(ClientPlugins {
        tick_duration: Duration::from_secs_f64(1.0 / 64.0),
    });
    
    // 2. Your protocol second — it registers into the lightyear registries above
    app.add_plugins(ProtocolPlugin);
    
    // 3. Everything else
    app.add_plugins(MyGamePlugin);
    
    app.run();
}
```

---

## Defining the Protocol

The protocol is the "contract" shared between client and server. Put it in `protocol.rs` and add it as a plugin on both sides.

### Components

Replicated components must be `Serialize + Deserialize + Clone`. Register each one in the `ProtocolPlugin`:

```rust
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerPosition(pub Vec2);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerHealth(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerColor(pub Color);

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<PlayerPosition>()
            .add_prediction()             // enable client-side prediction
            .add_linear_interpolation(); // enable interpolation for remote players

        app.register_component::<PlayerHealth>()
            .add_prediction();            // predict health changes from local actions

        app.register_component::<PlayerColor>(); // replicated but not predicted
    }
}
```

### Messages

Messages are one-off serializable values sent over the network. They use **lightyear's own registry** — do not confuse with Bevy's `Message` / `add_message` system (they are completely separate).

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub sender: String,
    pub text: String,
}

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Must specify direction — messages are directional
        app.register_message::<ChatMessage>()
            .add_direction(NetworkDirection::ServerToClient);
    }
}
```

> **Lightyear messages ≠ Bevy messages.** Lightyear uses `register_message` + `MessageSender`/`MessageReceiver`. Bevy's system uses `add_message` + `MessageWriter`/`MessageReader`. They are **entirely separate systems** — do not mix them.

### Channels

Channels define delivery guarantees. You only need custom channels if the defaults don't fit.

```rust
pub struct ReliableOrderedChannel;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_channel::<ReliableOrderedChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
    }
}
```

Channel modes:
- `OrderedReliable` — guaranteed delivery, in-order (chat, game events)
- `SequencedUnreliable` — latest wins, drops old (fast-moving positions)
- `UnorderedUnreliable` — fire-and-forget (high-frequency telemetry)

### Inputs

Player inputs are tick-synced: input for tick N arrives on the server in time for tick N.

```rust
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, Reflect)]
pub enum PlayerInput {
    #[default]
    None,
    Move { direction: Vec2 },
    Jump,
}

// If the input references entities, implement MapEntities
impl MapEntities for PlayerInput {
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(input::native::InputPlugin::<PlayerInput>::default());
        // Or with leafwing: app.add_plugins(LeafwingInputPlugin::<MyAction>::default());
    }
}
```

---

## Deciding What to Replicate, Predict, and Interpolate

This is the most important design decision per component. Get it wrong and you'll have lag, jitter, or wasted bandwidth.

| Strategy | What it means | When to use it |
|----------|--------------|----------------|
| **Replicated only** | Server sends updates; client uses raw server state | Slow-changing or cosmetic data |
| **Predicted** | Client runs the same simulation locally; rolls back on mismatch | Anything that responds to the local player's input |
| **Interpolated** | Client smoothly blends between received server snapshots | Remote entities the local player does NOT control |
| **Not registered** | Never sent over the network | Pure client-side visuals/state |

### Per-data-type guide

| Component / data | Register? | Prediction? | Interpolation? | Reason |
|-----------------|-----------|-------------|----------------|--------|
| Local player position/velocity | ✅ | ✅ | ❌ | Controlled by local input — predict to hide latency |
| Local player health (from own hits) | ✅ | ✅ | ❌ | Respond immediately to local actions |
| Other players' position | ✅ | ❌ | ✅ | Don't have their inputs; interpolate for smoothness |
| Other players' health | ✅ | ❌ | ❌ | Only server updates matter; no need for interpolation |
| Player name / color | ✅ | ❌ | ❌ | Static/cosmetic; raw server value is fine |
| Score / inventory | ✅ | ❌ | ❌ | Authoritative, no need for visual smoothing |
| AI entity position | ✅ | ❌ | ✅ | AI runs on server; interpolate for smoothness |
| Projectile (spawned by server) | ✅ | ❌ | ✅ | Server-controlled; interpolate |
| Projectile (pre-spawned by client) | ✅ | ✅ | ❌ | Use pre-spawn prediction for immediate feedback |
| `Transform` / mesh handles | ❌ | — | — | Compute from replicated data on the client |
| UI state, local VFX | ❌ | — | — | Purely local |

### Key rules

- **Predict anything driven by local input.** The prediction/rollback loop requires the client to re-simulate from stored inputs — both sides must produce identical results (no random calls, no timestamps in logic).
- **Interpolate remote entities.** Gives smooth visuals without needing their inputs.
- **Don't predict what you can't roll back.** Irreversible side effects (spawning effects, playing sounds) should be gated on `Confirmed` state, not `Predicted` state.
- **Keep non-replicated components off the protocol.** Anything derived from network state (visual `Transform`, material colors) should be computed locally, not sent over the wire.

---

## Server Setup

> **Connection events use observers, not EventReader.** Lightyear 0.26 uses Bevy's observer pattern (`On<Add, Component>`) for connection lifecycle — never `EventReader<ConnectEvent>` or similar. The two key observers are `On<Add, LinkOf>` (raw link established) and `On<Add, Connected>` (authentication complete).

```rust
use lightyear::prelude::server::*;
use lightyear::prelude::*;

fn setup_server(mut commands: Commands) {
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000);
    commands.spawn((
        NetcodeServer::new(NetcodeConfig::default()),
        LocalAddr(server_addr),
        ServerUdpIo::default(),
    ));
    // trigger Start on the spawned entity to begin listening
    // commands.trigger_targets(Start, server_entity);
}

// Observer: called when a new raw link is established with a client.
// Use this to add ReplicationSender (enables replicating entities to this client).
pub fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        ReplicationSender::new(
            Duration::from_millis(100), // how often to send replication updates
            SendUpdatesMode::SinceLastAck,
            false,
        ),
        Name::from("Client"),
    ));
}

// Observer: called only after authentication succeeds — safe to start game logic here.
pub fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    let Ok(client_id) = query.get(trigger.entity) else { return };
    let client_id = client_id.0;

    commands.spawn((
        PlayerBundle::new(client_id, Vec2::ZERO),
        Replicate::to_clients(NetworkTarget::All),
        // Controlling client predicts; others interpolate
        PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
        // Tie the player entity lifetime to the client connection
        ControlledBy {
            owner: trigger.entity,
            lifetime: Default::default(),
        },
    ));
}

pub struct MyServerPlugin;
impl Plugin for MyServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(handle_new_client);
        app.add_observer(handle_connected);
        // Simulation must run in FixedUpdate
        app.add_systems(FixedUpdate, server_movement);
    }
}
```

---

## Client Setup

```rust
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use lightyear::prelude::input::native::*;

fn setup_client(mut commands: Commands) {
    let client = commands.spawn((
        Client::default(),
        LocalAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)),
        PeerAddr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5000)),
        Link::new(None),
        ReplicationReceiver::default(),
        NetcodeClient::new(
            Authentication::Manual {
                server_addr: SERVER_ADDR,
                client_id: MY_CLIENT_ID,
                private_key: Key::default(),
                protocol_id: 0,
            },
            NetcodeConfig::default(),
        ).unwrap(),
        UdpIo::default(),
    )).id();
    commands.trigger_targets(Connect, client);
}

pub struct MyClientPlugin;
impl Plugin for MyClientPlugin {
    fn build(&self, app: &mut App) {
        // Input buffering must happen in FixedPreUpdate / WriteClientInputs set
        app.add_systems(
            FixedPreUpdate,
            buffer_input.in_set(InputSystems::WriteClientInputs),
        );
        // Apply inputs to predicted entities in FixedUpdate
        app.add_systems(FixedUpdate, client_movement);
        // React to predicted entity spawns
        app.add_observer(handle_predicted_spawn);
    }
}
```

---

## Input Handling

### Buffering inputs (client, FixedPreUpdate)

```rust
pub fn buffer_input(
    mut query: Query<&mut ActionState<PlayerInput>, With<InputMarker<PlayerInput>>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if let Ok(mut action_state) = query.single_mut() {
        let direction = Vec2::new(
            (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32,
            (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32,
        );
        action_state.0 = if direction != Vec2::ZERO {
            PlayerInput::Move { direction: direction.normalize() }
        } else {
            PlayerInput::None
        };
    }
}
```

- `InputMarker<I>` identifies the **locally-controlled** entity. Remote players may also have `ActionState<I>` (so you can observe their inputs) but without `InputMarker`.
- Add `InputMarker` in an observer when the `Predicted` entity spawns.

### Reading inputs (server + client predicted, FixedUpdate)

```rust
// Shared function — must be identical on both sides for prediction to work
pub fn shared_movement_behaviour(mut pos: Mut<PlayerPosition>, input: &ActionState<PlayerInput>) {
    if let PlayerInput::Move { direction } = input.0 {
        pos.0 += direction * MOVE_SPEED;
    }
}

// Server: run on all player entities (excluding Predicted entities in host-server mode)
fn server_movement(
    mut query: Query<(&mut PlayerPosition, &ActionState<PlayerInput>), Without<Predicted>>,
) {
    for (pos, input) in query.iter_mut() {
        shared_movement_behaviour(pos, input);
    }
}

// Client: only run on locally-predicted entities
fn client_movement(
    mut query: Query<(&mut PlayerPosition, &ActionState<PlayerInput>), With<Predicted>>,
) {
    for (pos, input) in query.iter_mut() {
        shared_movement_behaviour(pos, input);
    }
}
```

---

## Messaging

### Sending from server

```rust
fn broadcast_chat(
    mut sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let msg = ChatMessage { sender: "Server".into(), text: "Hello!".into() };
    sender
        .send::<_, ReliableOrderedChannel>(&msg, server.into_inner(), &NetworkTarget::All)
        .ok();
}
```

### Receiving on client

```rust
fn receive_chat(mut receiver: Single<&mut MessageReceiver<ChatMessage>>) {
    for msg in receiver.receive() {
        info!("[{}] {}", msg.sender, msg.text);
    }
}
```

---

## Entity References in Components

If a replicated component contains an `Entity` field, you **must** implement `MapEntities` and register it, or entity IDs will be wrong on the remote side. Server and client are separate ECS worlds — entity IDs don't match across the network boundary, and lightyear uses a mapping table to translate them.

```rust
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OwnedBy(pub Entity);  // references another entity

impl MapEntities for OwnedBy {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        // Use get_mapped (not map_entity) to translate the server entity ID
        // into the corresponding client entity ID
        self.0 = mapper.get_mapped(self.0);
    }
}

// In ProtocolPlugin — two separate calls, not chained
app.register_component::<OwnedBy>();
app.add_map_entities::<OwnedBy>(); // must be a separate call, not .add_map_entities() chained on register_component
```

> The correct method is `mapper.get_mapped(entity)`. Do not use `mapper.map_entity(entity)` — that method doesn't exist on `EntityMapper` in this context.

---

## Common Gotchas

### 1. Simulation outside FixedUpdate
Physics, movement, prediction, and rollback only work correctly in `FixedUpdate` (or `FixedPreUpdate`/`FixedPostUpdate`). Running simulation logic in `Update` will desync client and server.

### 2. Non-determinism breaks prediction
Rollback re-simulates from a checkpoint using stored inputs. Any non-determinism (random numbers, wall-clock time, floating-point ordering differences) will cause divergence and constant rollbacks.
```rust
// ❌ Non-deterministic
pos.0 += rand::random::<Vec2>() * speed;

// ✅ Deterministic — driven only by inputs
if let PlayerInput::Move { direction } = input.0 {
    pos.0 += direction * speed;
}
```

### 3. Missing direction on messages
Messages without a registered direction are silently dropped.
```rust
// ❌ Direction missing
app.register_message::<MyMsg>();

// ✅
app.register_message::<MyMsg>()
    .add_direction(NetworkDirection::ServerToClient);
```

### 4. Acting before `Connected`
`LinkOf` fires when the raw IO link is established. `Connected` fires after Netcode authentication succeeds. **Only start game logic (spawning, replication) when `Connected` is added.**

### 5. Forgetting `ReplicationSender` on the client link entity
Without `ReplicationSender` on the per-client link entity (added in the `handle_new_client` observer), no replication updates are sent to that client.

### 6. Not adding `InputMarker` to the predicted entity
Without `InputMarker<I>`, `buffer_input` has no entity to write to, and inputs won't be sent to the server.

### 8. Old prediction/interpolation API
Lightyear 0.26 does **not** use `ShouldBePredicted`, `ComponentSyncMode::Full`, or `#[derive(Channel)]`. The correct API is:
```rust
// ❌ Old/wrong
app.register_component::<Pos>(ChannelDirection::ServerToClient)
    .add_prediction(ComponentSyncMode::Full);

// ✅ Correct for 0.26
app.register_component::<Pos>()
    .add_prediction()
    .add_linear_interpolation();
```
Similarly, `ShouldBePredicted` doesn't control prediction targets — use `PredictionTarget` and `InterpolationTarget` on the entity instead.
| System | Register | Write | Read |
|--------|----------|-------|------|
| **Lightyear** | `app.register_message::<T>()` | `MessageSender<T>` | `MessageReceiver<T>` |
| **Bevy** | `app.add_message::<T>()` | `MessageWriter<T>` | `MessageReader<T>` |
These are completely separate — do not mix them.

---

## Reference Files

For deeper detail, read:
- `references/replication.md` — prediction/rollback mechanics, interpolation, pre-spawning, authority transfer, interest management
- `references/protocol.md` — full protocol registration patterns, channel modes, input plugin variants (native, leafwing, bevy-enhanced-input)
