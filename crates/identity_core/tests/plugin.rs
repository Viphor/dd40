use bevy::prelude::*;
use dd40_identity_core::{AuthConfig, IdentityCorePlugin};

#[test]
fn identity_core_plugin_inserts_auth_config() {
    let mut app = App::new();
    app.add_plugins(IdentityCorePlugin);
    app.update();
    assert!(
        app.world().get_resource::<AuthConfig>().is_some(),
        "AuthConfig resource should be inserted by IdentityCorePlugin"
    );
}

#[test]
fn default_auth_config_has_sensible_timeout() {
    let cfg = AuthConfig::default();
    assert_eq!(cfg.auth_timeout_secs, 5);
    assert!(cfg.token_file.is_empty());
    assert!(cfg.jwks_uri.is_empty());
}
