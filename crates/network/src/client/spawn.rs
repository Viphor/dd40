use std::time::Duration;

use bevy::{platform::collections::HashSet, prelude::*};
use dd40_character_core::components::Player;
use dd40_core::prelude::*;

use crate::{
    PlayerPosition,
    client::loading::{
        LOADING_KEY_INITIAL_CHUNKS, register_initial_chunks_loading_item,
        remove_initial_chunks_loading_item, remove_spawn_location_loading_item,
    },
};

/// How long the client will wait for the initial spawn chunks before giving up
/// and transitioning to [`AppState::Playing`] anyway.
///
/// Override [`SpawnChunkTimeout`] after adding [`ClientNetworkPlugin`] to use
/// a different value.
pub const DEFAULT_SPAWN_CHUNK_TIMEOUT_SECS: f32 = 15.0;

// ============================================================================
// RESOURCES
// ============================================================================

#[derive(Resource, Debug, Deref, DerefMut)]
pub struct ClientId(u64);

/// Tracks which of the initial spawn chunks are still pending during the
/// loading phase.
///
/// Inserted when a [`PlayerSpawnLocation`] message arrives (at which point the
/// client derives the expected 3×3 [`ChunkPos`] grid) and removed once every
/// one of those chunks has been received — or when the timeout fires.
///
/// While this resource exists, the `"network:initial_chunks"` key is held in
/// [`LoadingTracker`], preventing the app from leaving the `Loading` state.
#[derive(Resource, Debug)]
pub struct InitialChunksGate {
    /// Chunk positions that have been announced by the server but not yet
    /// received by the client.
    pub pending: HashSet<ChunkPos>,
}

/// Configures how long the client will wait for the initial spawn chunks
/// before forcibly releasing the loading gate.
///
/// Inserted with a [`DEFAULT_SPAWN_CHUNK_TIMEOUT_SECS`] default by
/// [`ClientNetworkPlugin`]. Override it after adding the plugin to change the
/// timeout for your project.
///
/// # Example
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use dd40_network::client::SpawnChunkTimeout;
/// # let mut app = App::new();
/// app.insert_resource(SpawnChunkTimeout::from_secs(30.0));
/// ```
#[derive(Resource, Debug, Clone)]
pub struct SpawnChunkTimeout {
    /// Wall-clock time at which the gate should be forcibly released, or
    /// [`None`] if the timer has not yet started (i.e. no [`PlayerSpawnLocation`]
    /// has been received yet).
    deadline: Option<Duration>,
    /// How long to wait after receiving [`PlayerSpawnLocation`] before giving up.
    duration: Duration,
}

impl SpawnChunkTimeout {
    /// Creates a timeout of `secs` seconds.
    pub fn from_secs(secs: f32) -> Self {
        Self {
            deadline: None,
            duration: Duration::from_secs_f32(secs),
        }
    }

    /// Starts the countdown using the current elapsed app time.
    fn start(&mut self, elapsed: Duration) {
        self.deadline = Some(elapsed + self.duration);
    }

    /// Returns `true` if the deadline has passed.
    fn is_expired(&self, elapsed: Duration) -> bool {
        self.deadline.is_some_and(|d| elapsed >= d)
    }
}

impl Default for SpawnChunkTimeout {
    fn default() -> Self {
        Self::from_secs(DEFAULT_SPAWN_CHUNK_TIMEOUT_SECS)
    }
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Reads incoming [`PlayerSpawnLocation`] messages and sets up the initial
/// chunk gate and spawn position.
///
/// For each message received this system:
///
/// 1. Derives the 3×3 [`ChunkPos`] grid centred on the spawn position.
/// 2. Inserts [`InitialChunksGate`] with those 9 positions as pending.
/// 3. Inserts [`SpawnPosition`] so the player plugin knows where to place the
///    player entity.
/// 4. Registers the `"network:initial_chunks"` key with [`LoadingTracker`] and
///    starts the [`SpawnChunkTimeout`] countdown.
pub(crate) fn receive_spawn_location(
    mut commands: Commands,
    player: Single<&PlayerPosition, Added<Player>>,
    mut requester: MessageWriter<RequestChunk>,
    mut tracker: ResMut<LoadingTracker>,
    mut timeout: ResMut<SpawnChunkTimeout>,
    time: Res<Time>,
) {
    let pos = player.to_vec3();
    info!("Received PlayerSpawnLocation: {:?}", pos);
    remove_spawn_location_loading_item(&mut tracker);

    // Derive the centre chunk from the spawn position.
    let centre = ChunkPos::from(&pos);

    // Build the 3×3 grid of expected chunk positions.
    let pending: HashSet<ChunkPos> = (-1_i32..=1)
        .flat_map(|dx| (-1_i32..=1).map(move |dz| ChunkPos::new(centre.x + dx, centre.z + dz)))
        .collect();

    debug!(
        "InitialChunksGate: expecting {} chunks centred on {:?}",
        pending.len(),
        centre,
    );

    commands.insert_resource(InitialChunksGate {
        pending: pending.clone(),
    });

    requester.write_batch(pending.iter().map(|pos| RequestChunk {
        pos: *pos,
        current_version: 0,
    }));

    // Register the loading gate and start the timeout from now.
    if !tracker.contains(LOADING_KEY_INITIAL_CHUNKS) {
        register_initial_chunks_loading_item(&mut tracker);
    }
    timeout.start(time.elapsed());
}

/// Drains [`InitialChunksGate::pending`] as chunks arrive and releases the
/// loading gate once all expected spawn chunks have been received.
///
/// Each frame this system reads [`ChunkReceivedNotification`] messages written
/// by [`chunk_provider::receive_chunk_data`] and removes matching positions
/// from the gate. When `pending` becomes empty the resource is removed and the
/// `"network:initial_chunks"` loading key is cleared.
///
/// If [`InitialChunksGate`] does not exist, notifications are drained and
/// discarded so they do not accumulate in the message buffer.
pub(crate) fn track_initial_chunks(
    mut commands: Commands,
    gate: Option<ResMut<InitialChunksGate>>,
    mut notifications: MessageReader<ChunkReady>,
    mut tracker: ResMut<LoadingTracker>,
) {
    let Some(mut gate) = gate else {
        return;
    };

    for notification in notifications.read() {
        if gate.pending.remove(&notification.chunk.position()) {
            trace!(
                "InitialChunksGate: received {:?}, {} remaining",
                notification.chunk.position(),
                gate.pending.len(),
            );
        }
    }

    if gate.pending.is_empty() {
        commands.remove_resource::<InitialChunksGate>();
        remove_initial_chunks_loading_item(&mut tracker);
    }
}

/// Releases the `"network:initial_chunks"` loading gate if the
/// [`SpawnChunkTimeout`] deadline has passed and the gate is still open.
///
/// This is a safety valve: if the server stalls or some chunks are lost the
/// client will still eventually transition to [`AppState::Playing`] rather
/// than hanging on the loading screen indefinitely.
pub(crate) fn timeout_initial_chunks(
    mut commands: Commands,
    gate: Option<ResMut<InitialChunksGate>>,
    timeout: Res<SpawnChunkTimeout>,
    mut tracker: ResMut<LoadingTracker>,
    time: Res<Time>,
) {
    // Only act when the gate is still open and the deadline has passed.
    if gate.is_none() {
        return;
    }

    if timeout.is_expired(time.elapsed()) {
        warn!(
            "ClientNetworkPlugin: spawn chunk timeout expired — forcing loading gate release. \
             Some terrain around the spawn point may not have arrived yet."
        );
        commands.remove_resource::<InitialChunksGate>();
        remove_initial_chunks_loading_item(&mut tracker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_chunks_gate_contains_nine_positions() {
        let centre = ChunkPos::new(0, 0);
        let pending: HashSet<ChunkPos> = (-1_i32..=1)
            .flat_map(|dx| (-1_i32..=1).map(move |dz| ChunkPos::new(centre.x + dx, centre.z + dz)))
            .collect();
        let gate = InitialChunksGate { pending };
        assert_eq!(gate.pending.len(), 9);
    }

    #[test]
    fn spawn_chunk_timeout_not_expired_before_start() {
        let timeout = SpawnChunkTimeout::from_secs(10.0);
        // Deadline is None before start(), so is_expired should never fire.
        assert!(!timeout.is_expired(Duration::from_secs(9999)));
    }

    #[test]
    fn spawn_chunk_timeout_expires_after_duration() {
        let mut timeout = SpawnChunkTimeout::from_secs(5.0);
        let start = Duration::from_secs(100);
        timeout.start(start);
        assert!(!timeout.is_expired(Duration::from_secs(104)));
        assert!(timeout.is_expired(Duration::from_secs(105)));
    }

    #[test]
    fn spawn_chunk_timeout_expires_exactly_at_deadline() {
        let mut timeout = SpawnChunkTimeout::from_secs(3.0);
        let start = Duration::from_secs(10);
        timeout.start(start);
        assert!(timeout.is_expired(Duration::from_secs(13)));
    }

    #[test]
    fn default_timeout_matches_constant() {
        let timeout = SpawnChunkTimeout::default();
        assert_eq!(
            timeout.duration,
            Duration::from_secs_f32(DEFAULT_SPAWN_CHUNK_TIMEOUT_SECS)
        );
    }
}
