# Protocol Registration — Deep Reference

## Table of Contents
1. Component Registration Options
2. Message Registration Options
3. Channel Modes Reference
4. Input Plugin Variants
5. Complete Protocol Example

---

## 1. Component Registration Options

```rust
app.register_component::<MyComponent>()
    // Enable client-side prediction with rollback
    .add_prediction()
    
    // Enable snapshot interpolation (requires Ease impl or custom fn)
    .add_linear_interpolation()
    
    // OR provide a custom interpolation function
    .add_interpolation_fn(|start: MyComponent, end: MyComponent, t: f32| {
        MyComponent(start.0.lerp(end.0, t))
    })
    
    // Enable lag compensation (for FPS-style hit detection)
    .add_lag_compensation()
    
    // If the component contains Entity fields, register the entity mapper too
    ; // close register_component chain
app.add_map_entities::<MyComponent>(); // call separately
```

### Which options can combine?

| Combo | Valid? | Notes |
|-------|--------|-------|
| prediction only | ✅ | Most common for player-controlled state |
| interpolation only | ✅ | Most common for remote entities |
| prediction + interpolation | ✅ | Controlling client predicts; others interpolate |
| lag compensation | requires prediction | Used with FPS hit detection |
| none | ✅ | Pure replication, no smoothing |

---

## 2. Message Registration Options

```rust
app.register_message::<MyMessage>()
    .add_direction(NetworkDirection::ServerToClient)
    // Optional: send over a specific channel (default channel used if omitted)
    .add_channel::<MyChannel>();
```

### Direction values
- `NetworkDirection::ServerToClient` — server → all clients
- `NetworkDirection::ClientToServer` — all clients → server
- `NetworkDirection::Bidirectional` — both directions

### Receiving messages

On the client, a `MessageReceiver<MyMessage>` component is automatically added to the `Client` entity when the direction includes `ServerToClient`.

```rust
fn receive_my_message(mut receiver: Single<&mut MessageReceiver<MyMessage>>) {
    for msg in receiver.receive() {
        // handle msg
    }
}
```

On the server, a `MessageReceiver<MyMessage>` is automatically added to each `ClientOf` link entity when direction includes `ClientToServer`.

```rust
fn receive_from_clients(mut receivers: Query<&mut MessageReceiver<MyMessage>, With<ClientOf>>) {
    for mut receiver in receivers.iter_mut() {
        for msg in receiver.receive() {
            // handle msg
        }
    }
}
```

---

## 3. Channel Modes Reference

| Mode | Delivery | Ordering | Best for |
|------|----------|----------|----------|
| `UnorderedUnreliable` | May drop | None | High-frequency telemetry |
| `SequencedUnreliable` | May drop | Latest only | Position updates (stale = useless) |
| `UnorderedReliable` | Guaranteed | None | Independent events |
| `OrderedReliable` | Guaranteed | FIFO | Chat, game events, commands |

```rust
// Default channels provided by lightyear (no need to register these):
// - DefaultUnreliableChannel (SequencedUnreliable) — used for component updates
// - DefaultReliableChannel (OrderedReliable) — used for entity spawn/despawn events
```

Custom channel example:

```rust
pub struct FastPositionChannel;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_channel::<FastPositionChannel>(ChannelSettings {
            mode: ChannelMode::SequencedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
    }
}
```

---

## 4. Input Plugin Variants

### Native (keyboard/mouse/gamepad)

```rust
// In ProtocolPlugin
app.add_plugins(input::native::InputPlugin::<MyInput>::default());
```

- Write inputs in `FixedPreUpdate` with `.in_set(InputSystems::WriteClientInputs)`
- Read from `ActionState<MyInput>` component

### Leafwing Input Manager

```toml
[dependencies]
lightyear = { features = ["leafwing"] }
leafwing-input-manager = "0.x"
```

```rust
app.add_plugins(LeafwingInputPlugin::<MyAction>::default());
```

Leafwing actions are automatically networked. Use `ActionState<MyAction>` as usual.

### Bevy Enhanced Input

```toml
[dependencies]
lightyear = { features = ["bevy_enhanced_input"] }
```

```rust
app.add_plugins(BeiInputPlugin::<MyAction>::default());
```

---

## 5. Complete Protocol Example

```rust
use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

// ---- Components ----

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Deref, DerefMut)]
pub struct Position(pub Vec3);

impl Ease for Position {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            Position(Vec3::lerp(start.0, end.0, t))
        })
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Health(pub f32);

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerName(pub String);

// Component with entity reference — needs MapEntities
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CarryingItem(pub Entity);

impl MapEntities for CarryingItem {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.0 = mapper.get_mapped(self.0);
    }
}

// ---- Messages ----

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMsg {
    pub sender: String,
    pub text: String,
}

// ---- Channels ----

pub struct ChatChannel;
pub struct FastChannel;

// ---- Inputs ----

#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Clone, Reflect)]
pub enum PlayerAction {
    #[default]
    Idle,
    Move(Vec2),
    Jump,
    Attack,
}

impl MapEntities for PlayerAction {
    fn map_entities<M: EntityMapper>(&mut self, _: &mut M) {}
}

// ---- Plugin ----

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Components
        app.register_component::<Position>()
            .add_prediction()
            .add_linear_interpolation();

        app.register_component::<Health>()
            .add_prediction(); // predict HP changes from local hits

        app.register_component::<PlayerName>(); // replicated only, no prediction

        app.register_component::<CarryingItem>();
        app.add_map_entities::<CarryingItem>(); // entity reference — must map

        // Messages
        app.register_message::<ChatMsg>()
            .add_direction(NetworkDirection::ServerToClient)
            .add_channel::<ChatChannel>();

        // Channels
        app.add_channel::<ChatChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);

        app.add_channel::<FastChannel>(ChannelSettings {
            mode: ChannelMode::SequencedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        // Inputs
        app.add_plugins(input::native::InputPlugin::<PlayerAction>::default());
    }
}
```
