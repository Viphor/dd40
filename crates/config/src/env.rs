//! Env var overlay — layer 5 of the config system.
//!
//! Scans all environment variables for the `DD40_<SECTION>__<KEY>` pattern and
//! applies them as overrides on top of the file-merged table. Values are
//! auto-coerced to the most specific TOML type that fits.

use bevy::prelude::*;

/// Legacy aliases: maps an old env var name to its canonical `(section, key)`
/// equivalent. Each alias is logged at `warn!` once when applied.
const LEGACY_ALIASES: &[(&str, &str, &str)] = &[
    // ("ENV_VAR_NAME", "section", "key")
    ("DD40_PRIVATE_KEY", "network", "private_key"),
];

/// Apply all `DD40_*` env var overrides onto `table` (mutates in place).
pub(crate) fn apply_env_overrides(table: &mut toml::Table) {
    // Handle legacy aliases first.
    for &(env_var, section, key) in LEGACY_ALIASES {
        if let Ok(raw) = std::env::var(env_var) {
            warn!(
                env_var,
                canonical = %format!("DD40_{}__{}", section.to_uppercase(), key.to_uppercase()),
                "legacy env var; please migrate to the canonical form"
            );
            let val = coerce_value(&raw);
            let section_table = table
                .entry(section)
                .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                .as_table_mut()
                .expect("section must be a table");
            section_table.insert(key.to_string(), val);
        }
    }

    // Scan all env vars for DD40_<SECTION>__<KEY>.
    for (name, raw) in std::env::vars() {
        let Some(rest) = name.strip_prefix("DD40_") else {
            continue;
        };

        // Skip if it exactly matches a legacy alias (already handled above).
        if LEGACY_ALIASES.iter().any(|&(alias, _, _)| alias == name) {
            continue;
        }

        let Some((section_upper, key_upper)) = rest.split_once("__") else {
            continue;
        };

        let section = section_upper.to_lowercase();
        let key = key_upper.to_lowercase();
        let val = coerce_value(&raw);

        let section_table = table
            .entry(section)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .expect("section must be a table");
        section_table.insert(key, val);
    }
}

/// Parse `raw` into the most specific TOML scalar type:
/// 1. Bool  (`1|true|yes|on` / `0|false|no|off`, case-insensitive)
/// 2. Integer (`i64`)
/// 3. Float (`f64`)
/// 4. String (fallback)
fn coerce_value(raw: &str) -> toml::Value {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();

    if matches!(lower.as_str(), "1" | "true" | "yes" | "on") {
        return toml::Value::Boolean(true);
    }
    if matches!(lower.as_str(), "0" | "false" | "no" | "off") {
        return toml::Value::Boolean(false);
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        return toml::Value::Integer(i);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return toml::Value::Float(f);
    }
    toml::Value::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_booleans() {
        for v in ["1", "true", "TRUE", "yes", "Yes", "on", "ON"] {
            assert_eq!(coerce_value(v), toml::Value::Boolean(true), "input: {v}");
        }
        for v in ["0", "false", "FALSE", "no", "No", "off", "OFF"] {
            assert_eq!(coerce_value(v), toml::Value::Boolean(false), "input: {v}");
        }
    }

    #[test]
    fn coerce_integers() {
        assert_eq!(coerce_value("42"), toml::Value::Integer(42));
        assert_eq!(coerce_value("-7"), toml::Value::Integer(-7));
        // "2" is an integer, not a bool.
        assert_eq!(coerce_value("2"), toml::Value::Integer(2));
    }

    #[test]
    fn coerce_floats() {
        assert_eq!(coerce_value("3.14"), toml::Value::Float(3.14));
        assert_eq!(coerce_value("1.0"), toml::Value::Float(1.0));
    }

    #[test]
    fn coerce_string_fallback() {
        assert_eq!(
            coerce_value("hello"),
            toml::Value::String("hello".to_string())
        );
        assert_eq!(
            coerce_value("/some/path"),
            toml::Value::String("/some/path".to_string())
        );
    }

    #[test]
    fn apply_env_sets_section_key_from_env() {
        // We mutate the process environment, so run in isolation by using a
        // unique key that no other test or system would set.
        let env_key = "DD40_TESTONLY_CFGENV__MY_VALUE";
        // SAFETY: single-threaded test, no other thread reads this var.
        unsafe { std::env::set_var(env_key, "99") };
        let mut table = toml::Table::new();
        apply_env_overrides(&mut table);
        unsafe { std::env::remove_var(env_key) };
        assert_eq!(
            table["testonly_cfgenv"]["my_value"].as_integer(),
            Some(99)
        );
    }
}
