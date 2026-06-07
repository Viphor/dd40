use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use lightyear::netcode::PRIVATE_KEY_BYTES;

pub const SERVER_PORT: u16 = 6969;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), SERVER_PORT);
/// 0 means that the OS will assign any available port
pub const CLIENT_PORT: u16 = 0;
pub const SHARED_SETTINGS: SharedSettings = SharedSettings {
    protocol_id: 0,
    private_key: [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ],
};

#[derive(Copy, Clone, Debug)]
pub struct SharedSettings {
    /// An id to identify the protocol version
    pub protocol_id: u64,

    /// a 32-byte array to authenticate via the Netcode.io protocol
    pub private_key: [u8; 32],
}

/// Parse a private key from a comma-separated byte string (e.g. `"1,2,...,32"`).
///
/// Returns `Err` if any token is not a valid `u8` or the count is not exactly
/// [`PRIVATE_KEY_BYTES`].
pub fn parse_private_key_from_str(key_str: &str) -> Result<[u8; PRIVATE_KEY_BYTES], String> {
    let tokens: Vec<&str> = key_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let bytes: Vec<u8> = tokens
        .iter()
        .map(|s| {
            s.parse::<u8>()
                .map_err(|e| format!("invalid byte {s:?}: {e}"))
        })
        .collect::<Result<_, _>>()?;

    if bytes.len() != PRIVATE_KEY_BYTES {
        return Err(format!(
            "private key must have exactly {PRIVATE_KEY_BYTES} bytes, got {}",
            bytes.len()
        ));
    }

    let mut arr = [0u8; PRIVATE_KEY_BYTES];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}
