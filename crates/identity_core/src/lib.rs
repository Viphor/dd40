//! Foundation types for the dd40 identity system.
//!
//! Defines all shared types used by the client/server identity plugins
//! and the network bridge, without taking on any implementation dependencies.

pub mod auth_config;
pub mod components;
pub mod messages;
pub mod plugin;

pub use auth_config::{AccessList, AuthConfig};
pub use components::{Authenticated, AwaitingAuth, PlayerIdentity};
pub use messages::AuthTokenReceived;
pub use plugin::IdentityCorePlugin;
