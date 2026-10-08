//! The client's half of the HTTP/1.1 upgrade (RFC 6455, section 4.1),
//! sans-IO: the request a board writes to open the relay's device leg
//! (`GET /relay/device`), and the reading of the server's answer.
//!
//! The key is 16 random bytes from the caller's entropy, base64'd; the
//! answer must be `101` with `Upgrade: websocket`, `Connection: Upgrade` and
//! the `Sec-WebSocket-Accept` that key implies, or the connection is
//! refused.

use super::ws_accept_key::{accept_key, base64_encode};
use super::ws_handshake::MAX_REQUEST;

/// A client's `Sec-WebSocket-Key`: base64 of 16 random bytes, 24 chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientKey([u8; 24]);

impl ClientKey {
    /// A fresh key from `entropy` (5.1: a new nonce per connection).
    pub fn fresh(entropy: fn(&mut [u8])) -> Self {
        let mut nonce = [0u8; 16];
        entropy(&mut nonce);
        let mut out = [0u8; 24];
        base64_encode(&nonce, &mut out);
        Self(out)
    }

    /// The key as sent.
    pub fn as_str(&self) -> &str {
        // Base64's alphabet is ASCII.
        core::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// The `Sec-WebSocket-Accept` a server must answer it with.
    pub fn expected_accept(&self) -> [u8; 28] {
        accept_key(self.as_str())
    }
}

/// The request didn't fit the buffer it was written into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestTooLong;

/// Write the upgrade request for `path` on `host` (the `Host` header, with
/// `:port` when it is not 80) into `out`; its length.
pub fn client_request(
    out: &mut [u8],
    host: &str,
    path: &str,
    key: &ClientKey,
) -> Result<usize, RequestTooLong> {
    let parts: [&str; 7] = [
        "GET ",
        path,
        " HTTP/1.1\r\nHost: ",
        host,
        "\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: ",
        key.as_str(),
        "\r\nSec-WebSocket-Version: 13\r\n\r\n",
    ];
    let mut at = 0;
    for part in parts {
        let end = at + part.len();
        out.get_mut(at..end)
            .ok_or(RequestTooLong)?
            .copy_from_slice(part.as_bytes());
        at = end;
    }
    Ok(at)
}

/// What the start of a buffer holding the server's answer amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientHandshake {
    /// No `\r\n\r\n` yet: read more and ask again.
    Incomplete,
    /// The server upgraded. `consumed` is the response's length; any bytes
    /// after it are already WebSocket frames.
    Upgraded { consumed: usize },
    /// The server answered something else: this status (0 when the status
    /// line itself did not parse), or a `101` without the right headers.
    Refused { status: u16 },
}

/// Read the server's answer to `key` at the start of `buf`. Call again
/// with the grown buffer while it is [`ClientHandshake::Incomplete`]; past
/// [`MAX_REQUEST`] bytes with no end of headers it is refused.
pub fn parse_response(buf: &[u8], key: &ClientKey) -> ClientHandshake {
    let Some(end) = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
    else {
        return if buf.len() >= MAX_REQUEST {
            ClientHandshake::Refused { status: 0 }
        } else {
            ClientHandshake::Incomplete
        };
    };
    let Ok(head) = core::str::from_utf8(&buf[..end - 4]) else {
        return ClientHandshake::Refused { status: 0 };
    };
    let mut lines = head.split("\r\n");
    let mut status_line = lines.next().unwrap_or_default().splitn(3, ' ');
    let status = match (status_line.next(), status_line.next()) {
        (Some("HTTP/1.1"), Some(code)) => code.parse::<u16>().unwrap_or(0),
        _ => 0,
    };
    if status != 101 {
        return ClientHandshake::Refused { status };
    }
    let expected = key.expected_accept();
    let (mut upgrade, mut connection, mut accepted) = (false, false, false);
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return ClientHandshake::Refused { status };
        };
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("upgrade") {
            upgrade |= value.eq_ignore_ascii_case("websocket");
        } else if name.eq_ignore_ascii_case("connection") {
            connection |= value.split(',').any(|item| {
                item.trim_matches([' ', '\t'])
                    .eq_ignore_ascii_case("upgrade")
            });
        } else if name.eq_ignore_ascii_case("sec-websocket-accept") {
            accepted = value.as_bytes() == expected;
        }
    }
    if upgrade && connection && accepted {
        ClientHandshake::Upgraded { consumed: end }
    } else {
        ClientHandshake::Refused { status }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::string::String;

    use super::*;

    const KEY: ClientKey = ClientKey(*b"dGhlIHNhbXBsZSBub25jZQ==");

    #[test]
    fn the_request_is_what_a_server_accepts() {
        let mut out = [0u8; 512];
        let len = client_request(&mut out, "lightplayer.app", "/relay/device", &KEY).unwrap();
        let text = core::str::from_utf8(&out[..len]).unwrap();
        assert_eq!(
            text,
            "GET /relay/device HTTP/1.1\r\nHost: lightplayer.app\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
        let mut tiny = [0u8; 40];
        assert_eq!(
            client_request(&mut tiny, "lightplayer.app", "/relay/device", &KEY),
            Err(RequestTooLong)
        );
    }

    #[test]
    fn a_101_with_the_right_accept_upgrades_and_keeps_trailing_bytes() {
        let response = "HTTP/1.1 101 Switching Protocols\r\nupgrade: WebSocket\r\n\
                        Connection: keep-alive, Upgrade\r\n\
                        Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n";
        for cut in 0..response.len() {
            assert_eq!(
                parse_response(&response.as_bytes()[..cut], &KEY),
                ClientHandshake::Incomplete,
                "cut {cut}"
            );
        }
        let mut with_frame = String::from(response);
        with_frame.push_str("\u{2}");
        assert_eq!(
            parse_response(with_frame.as_bytes(), &KEY),
            ClientHandshake::Upgraded {
                consumed: response.len()
            }
        );
    }

    #[test]
    fn anything_else_is_refused_with_its_status() {
        let cases = [
            ("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n", 404),
            (
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Accept: AAAAAAAAAAAAAAAAAAAAAAAAAAA=\r\n\r\n",
                101,
            ),
            (
                "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n",
                101,
            ),
            ("garbage\r\n\r\n", 0),
        ];
        for (response, status) in cases {
            assert_eq!(
                parse_response(response.as_bytes(), &KEY),
                ClientHandshake::Refused { status },
                "{response}"
            );
        }
        let long = [b'x'; MAX_REQUEST];
        assert_eq!(
            parse_response(&long, &KEY),
            ClientHandshake::Refused { status: 0 }
        );
    }

    #[test]
    fn a_fresh_key_is_base64_of_sixteen_bytes() {
        fn entropy(buf: &mut [u8]) {
            buf.copy_from_slice(b"the sample nonce");
        }
        assert_eq!(ClientKey::fresh(entropy), KEY);
    }
}
