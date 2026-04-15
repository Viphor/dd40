# Replication, Prediction, and Interpolation — Deep Reference

## Table of Contents
1. How Replication Works
2. The `Replicate` Component
3. Client-Side Prediction
4. Snapshot Interpolation
5. Prediction vs. Interpolation Decision Tree
6. Pre-Spawned Predicted Entities
7. Authority Transfer
8. Interest Management (Rooms)
9. Lag Compensation

---

## 1. How Replication Works

Lightyear tracks which components have changed each tick and sends diffs to clients that have a `ReplicationSender` on their link entity. On the client side, `ReplicationReceiver` applies those diffs to the local world.

Only components **registered in the protocol** are ever replicated. All others are ignored silently.

---

## 2. The `Replicate` Component

Add `Replicate` to any entity on the server that you want sent to clients.

```rust
// Replicate to all connected clients
commands.spawn((
    MyBundle::default(),
    Replicate::to_clients(NetworkTarget::All),
));

// Replicate only to specific clients
commands.spawn((
    MyBundle::default(),
    Replicate::to_clients(NetworkTarget::Single(client_id)),
));

// Replicate to all except one client
commands.spawn((
    MyBundle::default(),
    Replicate::to_clients(NetworkTarget::AllExceptSingle(client_id)),
));
```

Entities with `Replicate` get a `Replicated` marker on the client side. Use `Added<Replicated>` to react when a new entity arrives.

### Controlling which components replicate

By default all registered components on the entity are replicated. You can override per-component:

```rust
commands.spawn((
    PlayerPosition(Vec2::ZERO),
    PlayerColor(Color::RED),
    Replicate::to_clients(NetworkTarget::All),
    // Only replicate PlayerPosition, not PlayerColor
    ReplicateHierarchy::default(),
));
```

---

## 3. Client-Side Prediction

Prediction eliminates the perception of input latency by running the simulation locally and correcting when the server disagrees.

### How it works internally

1. Client reads input at tick N → applies it to the `Predicted` entity immediately
2. Input is buffered and sent to the server
3. Server applies input at tick N → sends back the authoritative state
4. Client receives state → compares with its own `Predicted` state
5. If they differ → **rollback**: reset to confirmed state, replay all buffered ticks

The `Predicted` entity lives a few ticks **ahead** of the server (≥ 1 RTT worth), so inputs have time to travel to the server and be applied at the right tick.

### Enabling prediction for a component

```rust
// In ProtocolPlugin
app.register_component::<PlayerPosition>()
    .add_prediction();
```

### Enabling prediction for an entity (server side)

```rust
commands.spawn((
    MyBundle::default(),
    Replicate::to_clients(NetworkTarget::All),
    // Only the controlling client predicts
    PredictionTarget::to_clients(NetworkTarget::Single(client_id)),
));
```

The client will then have:
- The raw `Replicated` entity with `Confirmed<PlayerPosition>` (server state)
- A separate `Predicted` entity with `PlayerPosition` (predicted state)

Query `With<Predicted>` to get the entity that responds to local input.

### Requirements for correct prediction

- Simulation must run in `FixedUpdate` (lightyear ties ticks to `FixedMain`)
- Logic must be **deterministic**: same inputs → same outputs, always
- The **same** movement/simulation function must run on both client and server

---

## 4. Snapshot Interpolation

Interpolation makes remote entities (those you don't control) appear smooth even when server updates come infrequently (e.g., 10 Hz).

### How it works internally

The `Interpolated` entity lives a few ticks **behind** the latest confirmed state. Lightyear buffers recent confirmed snapshots and blends between them frame by frame.

### Enabling interpolation for a component

You must provide an interpolation function. If your type implements `Ease`, use the helper:

```rust
use bevy::math::*;

#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Deref, DerefMut)]
pub struct PlayerPosition(pub Vec2);

impl Ease for PlayerPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            PlayerPosition(Vec2::lerp(start.0, end.0, t))
        })
    }
}

// In ProtocolPlugin
app.register_component::<PlayerPosition>()
    .add_linear_interpolation(); // uses the Ease impl above
```

For custom logic:
```rust
app.register_component::<MyComponent>()
    .add_interpolation_fn(|start, end, t| {
        // custom blend
        MyComponent(start.0.lerp(end.0, t))
    });
```

### Enabling interpolation for an entity (server side)

```rust
commands.spawn((
    MyBundle::default(),
    Replicate::to_clients(NetworkTarget::All),
    // All clients except the controlling one see an interpolated copy
    InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(client_id)),
));
```

The interpolated entity has the `Interpolated` marker. Query `With<Interpolated>` to find it.

---

## 5. Prediction vs. Interpolation Decision Tree

```
Does the local player send inputs that affect this entity?
├── YES → Predict it (PredictionTarget for the controlling client)
│         Other clients interpolate it (InterpolationTarget for the rest)
└── NO  → Does smooth visual motion matter?
          ├── YES → Interpolate it
          └── NO  → Replicate only (no prediction, no interpolation)
                    (e.g., score, name, inventory)
```

---

## 6. Pre-Spawned Predicted Entities

For immediate feedback (e.g., spawning a projectile on button press), spawn a `Predicted` entity on the client before the server confirms it:

```rust
// Client: spawn immediately with ShouldBePredicted marker
commands.spawn((
    ProjectileBundle::default(),
    ShouldBePredicted,
    // Optional: set a hash so lightyear can match this to the server's spawned entity
    PreSpawnedPlayerObject::default(),
));
```

When the server spawns the matching entity and replicates it, lightyear matches them up and transfers authority without a visible pop.

---

## 7. Authority Transfer

You can dynamically transfer authority over an entity between server and client:

```rust
// Give a client authority over an entity
commands.entity(entity).insert(HasAuthority);

// Revoke client authority
commands.entity(entity).remove::<HasAuthority>();
```

Use case: a client picks up an object → gains local authority for responsive movement → drops it → server takes back authority.

---

## 8. Interest Management (Rooms)

For large worlds, only replicate entities relevant to each player. Use `Room`s:

```rust
// Server: create a room
let room = commands.spawn(Room).id();

// Add an entity to the room
commands.entity(some_entity).insert(RoomTarget { room });

// Add a client to the room (they receive all entities in this room)
commands.entity(client_link_entity).insert(RoomTarget { room });
```

Entities not sharing a room with a client are not replicated to them. This replaces `NetworkTarget::All` with fine-grained visibility.

---

## 9. Lag Compensation

Used when a predicted (local) entity needs to interact with an interpolated (remote) entity — common in FPS games where you shoot a remote player.

```rust
// Server: enable lag compensation on the component
app.register_component::<PlayerPosition>()
    .add_prediction()
    .add_lag_compensation();
```

Lightyear rewinds the interpolated entity's history to match the tick when the client fired, so hit detection is fair even under latency.
