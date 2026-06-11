use std::path::PathBuf;

use bevy::prelude::Resource;
use dd40_config::ConfigSection;
use serde::{Deserialize, Serialize};

/// Configuration section `[auth]`.
///
/// Client fields: `token_file`.
/// Server fields: `jwks_uri`, `issuer`, `audience`, `auth_timeout_secs`,
/// `allow`, `deny`.
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
#[serde(default)]
pub struct AuthConfig {
    /// Path to the JWT file on disk (client).
    ///
    /// Supports `~` expansion. Relative paths are resolved from the working
    /// directory. Empty string means "not configured".
    pub token_file: String,

    /// OIDC JWKS endpoint URL (server). Fetched once at startup.
    pub jwks_uri: String,

    /// Expected `iss` claim value (server).
    pub issuer: String,

    /// Expected `aud` claim value (server). `None` skips audience validation.
    pub audience: Option<String>,

    /// Seconds to wait for `AuthToken` before disconnecting the client (server).
    pub auth_timeout_secs: u64,

    /// Allow-list: who may connect. `Open` (the default) allows any verified
    /// token. An `Inline` list or `File` path restricts to the listed `sub`
    /// values.
    pub allow: AccessList,

    /// Deny-list: always refused, even if on the allow-list. `Open` (the
    /// default) blocks nobody.
    pub deny: AccessList,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_file: String::new(),
            jwks_uri: String::new(),
            issuer: String::new(),
            audience: None,
            auth_timeout_secs: 5,
            allow: AccessList::Open,
            deny: AccessList::Open,
        }
    }
}

impl ConfigSection for AuthConfig {
    const SECTION: &'static str = "auth";
}

/// Determines which `sub` values are permitted to connect.
///
/// TOML representations:
/// - Missing key or `allow = []` → `Open` (all verified tokens accepted).
/// - `allow = "/path/to/file.txt"` → `File` (one sub per line).
/// - `allow = ["sub1", "sub2"]` → `Inline` (explicit list).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(untagged)]
pub enum AccessList {
    /// No restriction — all verified tokens are accepted.
    #[default]
    Open,
    /// Path to a file containing one `sub` value per line.
    File(PathBuf),
    /// Explicit list of allowed/denied `sub` values.
    Inline(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_open() {
        let cfg: AuthConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.allow, AccessList::Open);
        assert_eq!(cfg.deny, AccessList::Open);
    }

    #[test]
    fn string_value_is_file() {
        let cfg: AuthConfig =
            toml::from_str(r#"allow = "/etc/dd40/allow.txt""#).unwrap();
        assert!(matches!(cfg.allow, AccessList::File(_)));
    }

    #[test]
    fn array_value_is_inline() {
        let cfg: AuthConfig =
            toml::from_str(r#"allow = ["sub1", "sub2"]"#).unwrap();
        assert!(matches!(cfg.allow, AccessList::Inline(ref v) if v.len() == 2));
    }

    #[test]
    fn empty_array_is_inline_empty() {
        let cfg: AuthConfig = toml::from_str(r#"allow = []"#).unwrap();
        assert!(matches!(cfg.allow, AccessList::Inline(ref v) if v.is_empty()));
    }

    #[test]
    fn round_trip() {
        let original = AuthConfig {
            jwks_uri: "http://localhost:8080/realms/dd40/protocol/openid-connect/certs"
                .to_string(),
            issuer: "http://localhost:8080/realms/dd40".to_string(),
            audience: Some("dd40".to_string()),
            auth_timeout_secs: 10,
            allow: AccessList::Inline(vec!["sub1".to_string()]),
            deny: AccessList::File("/etc/deny.txt".into()),
            ..Default::default()
        };

        let serialized = toml::to_string(&original).unwrap();
        let deserialized: AuthConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.jwks_uri, original.jwks_uri);
        assert_eq!(deserialized.issuer, original.issuer);
        assert_eq!(deserialized.audience, original.audience);
        assert_eq!(deserialized.auth_timeout_secs, original.auth_timeout_secs);
        assert_eq!(deserialized.allow, original.allow);
        assert_eq!(deserialized.deny, original.deny);
    }
}
