//! [`RawConfig`] — the merged TOML table exposed as a Bevy Resource.

use bevy::prelude::*;

use crate::ConfigSection;

/// The fully-merged TOML table from all config layers, including env var
/// overrides. Inserted by [`crate::ConfigPlugin`] in `PreStartup`.
///
/// Downstream crates call [`RawConfig::section`] to extract their typed config
/// struct. No changes to `dd40_config` are needed when adding a new section.
#[derive(Resource, Clone, Debug, Default)]
pub struct RawConfig(pub toml::Table);

impl RawConfig {
    /// Extract and deserialize section `T`.
    ///
    /// Returns `T::default()` when:
    /// - the section key is absent from the table, or
    /// - deserialization fails (logged at `warn!`).
    pub fn section<T: ConfigSection>(&self) -> T {
        let Some(value) = self.0.get(T::SECTION) else {
            return T::default();
        };
        match T::deserialize(value.clone()) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    section = T::SECTION,
                    error = %e,
                    "failed to deserialize config section; using defaults"
                );
                T::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
    #[serde(default)]
    struct TestCfg {
        value: i32,
        name: String,
    }

    impl Default for TestCfg {
        fn default() -> Self {
            Self {
                value: 42,
                name: "default".to_string(),
            }
        }
    }

    impl ConfigSection for TestCfg {
        const SECTION: &'static str = "test";
    }

    #[test]
    fn section_absent_returns_default() {
        let raw = RawConfig::default();
        let cfg = raw.section::<TestCfg>();
        assert_eq!(cfg, TestCfg::default());
    }

    #[test]
    fn section_present_returns_parsed_value() {
        let table: toml::Table = toml::from_str(r#"[test]
value = 99
name = "hello"
"#)
        .unwrap();
        let raw = RawConfig(table);
        let cfg = raw.section::<TestCfg>();
        assert_eq!(cfg.value, 99);
        assert_eq!(cfg.name, "hello");
    }

    #[test]
    fn section_partial_uses_defaults_for_missing_keys() {
        let table: toml::Table = toml::from_str(r#"[test]
value = 7
"#)
        .unwrap();
        let raw = RawConfig(table);
        let cfg = raw.section::<TestCfg>();
        assert_eq!(cfg.value, 7);
        assert_eq!(cfg.name, "default"); // serde default for missing key
    }
}
