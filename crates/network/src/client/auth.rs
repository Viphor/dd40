use bevy::prelude::*;
use dd40_identity_core::AuthConfig;
use lightyear::prelude::MessageSender;

use crate::protocol::{AuthToken, EventChannel};

/// Sends the client's JWT to the server immediately after the connection
/// entity gets its `MessageSender<AuthToken>` component.
///
/// Runs once per connection (the `Added` filter fires only on the first frame
/// the component exists). Reads the token path from `auth.token_file` in
/// config, reads the file, and sends the raw JWT over [`EventChannel`].
pub(crate) fn send_auth_token(
    mut senders: Query<&mut MessageSender<AuthToken>, Added<MessageSender<AuthToken>>>,
    config: Option<Res<AuthConfig>>,
) {
    for mut sender in &mut senders {
        let token_file = config.as_ref().map(|c| c.token_file.as_str()).unwrap_or("");

        let token = if token_file.is_empty() {
            warn!("auth.token_file is not configured — sending empty auth token");
            String::new()
        } else {
            let expanded = shellexpand::tilde(token_file);
            match std::fs::read_to_string(expanded.as_ref()) {
                Ok(content) => {
                    let trimmed = content.trim().to_string();
                    if trimmed.is_empty() {
                        warn!(path = %token_file, "auth token file is empty");
                    }
                    info!(path = %token_file, "read auth token from file");
                    trimmed
                }
                Err(e) => {
                    warn!(path = %token_file, error = %e, "failed to read auth token file");
                    String::new()
                }
            }
        };

        sender.send::<EventChannel>(AuthToken { token });
    }
}
