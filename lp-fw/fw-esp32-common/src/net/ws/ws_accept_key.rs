//! The `Sec-WebSocket-Accept` value (RFC 6455, section 4.2.2, step 5.4).

use super::sha1::Sha1;

/// The GUID RFC 6455 appends to the client's key before hashing.
const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The standard base64 alphabet (RFC 4648, section 4).
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `base64(SHA-1(key ++ GUID))`: the accept value for a client's
/// `Sec-WebSocket-Key` (as sent, surrounding whitespace already trimmed).
/// Always 28 ASCII bytes.
pub fn accept_key(sec_websocket_key: &str) -> [u8; 28] {
    let mut hasher = Sha1::new();
    hasher.update(sec_websocket_key.as_bytes());
    hasher.update(WS_GUID);
    let digest = hasher.finish();

    // 20 bytes = six whole 3-byte groups and a 2-byte tail with one `=`.
    let mut out = [b'='; 28];
    for (group, chars) in digest.chunks(3).zip(out.chunks_mut(4)) {
        let b = [
            group[0],
            *group.get(1).unwrap_or(&0),
            *group.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        // One output char per 6 bits, and one more than the group's bytes.
        for (i, c) in chars.iter_mut().take(group.len() + 1).enumerate() {
            *c = BASE64[(n >> (18 - 6 * i) & 0x3f) as usize];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6455_example_key() {
        // RFC 6455, section 1.3.
        assert_eq!(
            &accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            b"s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn matches_tungstenite() {
        for key in ["AQIDBAUGBwgJCgsMDQ4PEA==", "x3JJHMbDL1EzLkh9GBhXDw==", ""] {
            let expected = tungstenite::handshake::derive_accept_key(key.as_bytes());
            assert_eq!(
                accept_key(key).as_slice(),
                expected.as_bytes(),
                "key {key:?}"
            );
        }
    }
}
