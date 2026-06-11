use bevy::prelude::{Resource, info, warn};
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use jsonwebtoken::jwk::JwkSet;
use serde::{Deserialize, Serialize};

/// Claims subset we extract from the JWT.
///
/// Only the fields we actually use are listed here; unknown fields are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the stable, durable player identifier.
    pub sub: String,
    /// Preferred username — display name shown in-game.
    pub preferred_username: Option<String>,
    /// Issuer — must match `auth.issuer`.
    pub iss: String,
    /// Expiry (Unix timestamp) — validated automatically by `jsonwebtoken`.
    pub exp: u64,
    /// Audience (optional) — validated when `auth.audience` is configured.
    pub aud: Option<serde_json::Value>,
}

/// Cached JWKS decoding keys fetched at server startup.
#[derive(Resource, Default)]
pub struct JwksCache {
    /// Decoded public keys suitable for JWT signature verification.
    pub keys: Vec<DecodingKey>,
}

/// Fetches the JWKS document from `uri` and returns parsed decoding keys.
///
/// Returns an empty Vec on network or parse errors (with a `warn!`).
pub fn fetch_jwks(uri: &str) -> Vec<DecodingKey> {
    match ureq::get(uri).call() {
        Ok(resp) => match resp.into_json::<serde_json::Value>() {
            Ok(json) => parse_jwks(&json),
            Err(e) => {
                warn!(error = %e, "failed to parse JWKS response as JSON");
                vec![]
            }
        },
        Err(e) => {
            warn!(error = %e, uri, "failed to fetch JWKS");
            vec![]
        }
    }
}

/// Parses a JWKS JSON document and returns `DecodingKey`s for every usable key.
pub fn parse_jwks(jwks: &serde_json::Value) -> Vec<DecodingKey> {
    let jwk_set: JwkSet = match serde_json::from_value(jwks.clone()) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to parse JWKS document");
            return vec![];
        }
    };

    let mut result = Vec::new();
    for jwk in &jwk_set.keys {
        match DecodingKey::from_jwk(jwk) {
            Ok(k) => result.push(k),
            Err(e) => warn!(error = %e, "skipping unparseable JWK entry"),
        }
    }

    info!(count = result.len(), "parsed JWKS keys");
    result
}

/// Verifies a raw JWT string against the cached keys and config parameters.
///
/// Returns the decoded [`Claims`] on success, or a `jsonwebtoken::errors::Error`
/// on failure (expired, bad signature, wrong issuer, etc.).
pub fn verify(
    token: &str,
    keys: &[DecodingKey],
    issuer: &str,
    audience: Option<&str>,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    if keys.is_empty() {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidKeyFormat,
        ));
    }

    // Peek at the header to find the key-id and algorithm.
    let header = decode_header(token)?;
    let alg = header.alg;

    let mut validation = Validation::new(alg);
    validation.set_issuer(&[issuer]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);

    if let Some(aud) = audience {
        validation.set_audience(&[aud]);
    } else {
        validation.validate_aud = false;
    }

    // Try each key; return the first successful decode.
    let mut last_err = None;
    for key in keys {
        match decode::<Claims>(token, key, &validation) {
            Ok(data) => return Ok(data.claims),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken)
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    use super::*;

    /// Generate a test RS256 key pair and return `(encoding_key, decoding_key)`.
    fn test_rsa_pair() -> (EncodingKey, DecodingKey) {
        // Use a pre-generated 2048-bit RSA key for determinism in tests.
        // Generated with: openssl genrsa 2048
        // This is a test-only key; never use in production.
        const PRIVATE_KEY_PEM: &str = include_str!("../tests/test_rsa_private.pem");
        const PUBLIC_KEY_PEM: &str = include_str!("../tests/test_rsa_public.pem");

        let enc = EncodingKey::from_rsa_pem(PRIVATE_KEY_PEM.as_bytes()).unwrap();
        let dec = DecodingKey::from_rsa_pem(PUBLIC_KEY_PEM.as_bytes()).unwrap();
        (enc, dec)
    }

    fn mint_token(
        enc: &EncodingKey,
        sub: &str,
        iss: &str,
        exp_offset_secs: i64,
        aud: Option<&str>,
    ) -> String {
        let now = jsonwebtoken::get_current_timestamp() as i64;
        let mut claims = BTreeMap::new();
        claims.insert("sub", serde_json::json!(sub));
        claims.insert("iss", serde_json::json!(iss));
        claims.insert("exp", serde_json::json!(now + exp_offset_secs));
        claims.insert("iat", serde_json::json!(now));
        if let Some(a) = aud {
            claims.insert("aud", serde_json::json!(a));
        }
        encode(&Header::new(Algorithm::RS256), &claims, enc).unwrap()
    }

    #[test]
    fn valid_token_passes() {
        let (enc, dec) = test_rsa_pair();
        let token = mint_token(&enc, "sub1", "https://issuer.example", 3600, None);
        let result = verify(&token, &[dec], "https://issuer.example", None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert_eq!(result.unwrap().sub, "sub1");
    }

    #[test]
    fn expired_token_fails() {
        let (enc, dec) = test_rsa_pair();
        // Use -3600 to ensure we're well past any leeway window.
        let token = mint_token(&enc, "sub1", "https://issuer.example", -3600, None);
        let result = verify(&token, &[dec], "https://issuer.example", None);
        assert!(result.is_err());
        let kind = result.unwrap_err().kind().clone();
        assert!(
            matches!(kind, jsonwebtoken::errors::ErrorKind::ExpiredSignature),
            "expected ExpiredSignature, got {:?}",
            kind
        );
    }

    #[test]
    fn wrong_issuer_fails() {
        let (enc, dec) = test_rsa_pair();
        let token = mint_token(&enc, "sub1", "https://other.example", 3600, None);
        let result = verify(&token, &[dec], "https://issuer.example", None);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_audience_fails() {
        let (enc, dec) = test_rsa_pair();
        let token = mint_token(&enc, "sub1", "https://issuer.example", 3600, Some("other"));
        let result = verify(&token, &[dec], "https://issuer.example", Some("dd40"));
        assert!(result.is_err());
    }

    #[test]
    fn correct_audience_passes() {
        let (enc, dec) = test_rsa_pair();
        let token = mint_token(&enc, "sub1", "https://issuer.example", 3600, Some("dd40"));
        let result = verify(&token, &[dec], "https://issuer.example", Some("dd40"));
        assert!(result.is_ok());
    }

    #[test]
    fn no_keys_fails() {
        let (enc, _) = test_rsa_pair();
        let token = mint_token(&enc, "sub1", "https://issuer.example", 3600, None);
        let result = verify(&token, &[], "https://issuer.example", None);
        assert!(result.is_err());
    }
}
