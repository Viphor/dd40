use dd40_config::ConfigSection;
use serde::{Deserialize, Serialize};

/// Client-side network configuration loaded from `[client]` in `config.toml`.
///
/// All keys are optional; missing keys fall back to their `Default`.
///
/// ```toml
/// [client]
/// server_host = "127.0.0.1"
/// server_port = 6969
/// ```
///
/// Env var overrides follow the standard `DD40_CLIENT__<KEY>` pattern:
///
/// ```bash
/// DD40_CLIENT__SERVER_HOST=192.168.1.10
/// DD40_CLIENT__SERVER_PORT=7000
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// IP address or hostname of the server to connect to.
    pub server_host: String,
    /// UDP port the server is listening on.
    pub server_port: u16,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_host: "127.0.0.1".to_string(),
            server_port: 6969,
        }
    }
}

impl ConfigSection for ClientConfig {
    const SECTION: &'static str = "client";
}
