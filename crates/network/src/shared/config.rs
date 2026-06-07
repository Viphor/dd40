use dd40_config::ConfigSection;
use serde::{Deserialize, Serialize};

/// Unified network configuration for both the server and client plugins.
///
/// Loaded from the `[network]` table in `config.toml` and overridable via
/// `DD40_NETWORK__<KEY>` environment variables.
///
/// ```toml
/// [network]
/// host             = "127.0.0.1"  # client: server address to connect to
/// port             = 6969         # server: listen port; client: connect port
/// private_key      = ""           # Netcode.io auth key (32 comma-separated bytes)
/// render_distance  = 8            # server: chunk broadcast radius in chunks
/// ```
///
/// All keys are optional; missing keys fall back to their `Default`.
///
/// The legacy env var `DD40_PRIVATE_KEY` is still accepted and remapped by
/// `dd40_config` to `DD40_NETWORK__PRIVATE_KEY`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// IP address or hostname of the server.
    ///
    /// Used by the client to establish a connection.  The server always binds
    /// to `0.0.0.0` (all interfaces) regardless of this setting.
    pub host: String,
    /// UDP port used by both the server (bind) and the client (connect).
    pub port: u16,
    /// Netcode.io private key as a comma-separated list of 32 unsigned bytes
    /// (e.g. `"1,2,3,...,32"`).  Empty string → use the built-in all-zero key.
    pub private_key: String,
    /// Chebyshev radius in chunks within which connected clients receive
    /// chunk-update broadcasts.
    pub render_distance: i32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 6969,
            private_key: String::new(),
            render_distance: 8,
        }
    }
}

impl ConfigSection for NetworkConfig {
    const SECTION: &'static str = "network";
}
