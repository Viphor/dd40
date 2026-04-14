//! Network communication layer for dd40 using the lightyear networking library.
//!
//! This crate provides client-server networking functionality for the dd40 voxel game.
//! It uses lightyear for reliable replication of entities, components, and messages
//! between clients and servers.
//!
//! # Architecture
//!
//! The networking layer is split into three main modules:
//!
//! - [`protocol`] - Shared protocol definitions (messages, components, channels)
//! - [`client`] - Client-side networking (connection, input sending, state reception)
//! - [`server`] - Server-side networking (connections, input processing, state replication)
//!
//! # Usage
//!
//! ## Client Setup
//!
//! Add the `ClientNetworkPlugin` to your client app:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use dd40_network::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(ClientNetworkPlugin)
//!         .run();
//! }
//! ```
//!
//! ## Server Setup
//!
//! Add the `ServerNetworkPlugin` to your server app:
//!
//! ```rust,no_run
//! use bevy::prelude::*;
//! use dd40_network::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(MinimalPlugins)
//!         .add_plugins(ServerNetworkPlugin(DDServer::new(SERVER_PORT)))
//!         .run();
//! }
//! ```
//!
//! # Protocol
//!
//! The networking protocol is defined in the [`protocol`] module and includes:
//!
//! - **Inputs**: Player input sent from client to server every frame
//! - **Messages**: Events like block changes, chunk data, player join/leave
//! - **Components**: Replicated components like player position, rotation, speed
//! - **Channels**: Network channels with different reliability guarantees
//!
//! # Replication
//!
//! Player entities are automatically replicated from server to clients. When a client
//! connects, the server spawns a player entity and marks it for replication. Changes
//! to replicated components (position, rotation, etc.) are automatically synchronized.
//!
//! Block changes are broadcast as messages rather than component replication, since
//! blocks are static and numerous.
//!
//! # Client-Side Prediction
//!
//! Client input is sent to the server, which performs authoritative simulation.
//! The server sends back the confirmed state, which is used to correct any prediction
//! errors on the client.

#[cfg(feature = "client")]
pub mod client;
pub mod connection;
pub mod constants;
pub mod protocol;
#[cfg(feature = "server")]
pub mod server;

// Re-export commonly used types
pub use protocol::{
    PlaceBlockRequest, PlayerInput, PlayerJoinedMessage, PlayerLeftMessage, PlayerPosition,
    PlayerRotation, PlayerSpawnLocation, PlayerSpeed, ProtocolPlugin,
};

#[cfg(feature = "client")]
pub use client::ClientNetworkPlugin;
#[cfg(feature = "client")]
pub use connection::{client::DDClient, shared::CLIENT_PORT};

#[cfg(feature = "server")]
pub use connection::{
    server::DDServer,
    shared::{SERVER_ADDR, SERVER_PORT},
};
#[cfg(feature = "server")]
pub use server::ServerNetworkPlugin;
#[cfg(feature = "server")]
pub use server::spawn::{PlayerLocations, WorldSpawnConfig};

/// Helper functions for coordinate conversions
pub use protocol::{chunk_local_to_global, global_to_chunk_local};
