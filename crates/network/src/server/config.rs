//! Config sections for the server-side network plugin.

use serde::{Deserialize, Serialize};

/// Config section for [`super::ServerNetworkPlugin`].
///
/// Read from the `[network]` table in `config.toml` and overridable via
/// `DD40_NETWORK__*` environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Chebyshev radius in chunks within which connected clients receive
    /// chunk-update broadcasts. Default: `8`.
    pub render_distance: i32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self { render_distance: 8 }
    }
}

impl dd40_config::ConfigSection for NetworkConfig {
    const SECTION: &'static str = "network";
}

/// Config section for server connection settings.
///
/// Read from the `[server]` table in `config.toml` and overridable via
/// `DD40_SERVER__*` environment variables. The legacy env var
/// `DD40_PRIVATE_KEY` is also accepted (remapped by `dd40_config`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// UDP port the server listens on. Default: `6969`.
    pub port: u16,
    /// Netcode.io private key as a comma-separated list of 32 unsigned
    /// bytes (e.g. `"1,2,3,...,32"`). Empty string → all-zero key.
    pub private_key: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 6969,
            private_key: String::new(),
        }
    }
}

impl dd40_config::ConfigSection for ServerConfig {
    const SECTION: &'static str = "server";
}
