//! The HTTP/1.1 upgrade request (RFC 6455, section 4.2), sans-IO.
//!
//! The board serves exactly one route, `GET /link`, and only as a
//! WebSocket: there is no other HTTP here. [`parse_request`] reads the
//! request out of a byte buffer the caller fills; [`upgrade_response`] and
//! [`Refusal::response`] are the only answers it can give.

use super::ws_accept_key::accept_key;

/// The one path the board upgrades.
pub const LINK_PATH: &str = "/link";

/// The longest request accepted, terminator included. Browsers send about
/// 500 bytes; anything longer is refused with 431.
pub const MAX_REQUEST: usize = 1024;

/// What a buffer holding the start of a request amounts to so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handshake<'a> {
    /// No `\r\n\r\n` yet: read more and ask again.
    Incomplete,
    /// A valid upgrade to [`LINK_PATH`]. `consumed` is the request's length;
    /// any bytes after it are already WebSocket frames.
    Upgrade { key: &'a str, consumed: usize },
    /// Answer with [`Refusal::response`] and close.
    Refuse { refusal: Refusal, consumed: usize },
}

/// Why a request was refused; each maps to one short HTTP response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// 400: not a well-formed WebSocket upgrade (wrong method, missing
    /// `Upgrade`/`Connection`, missing or malformed key, …).
    BadRequest,
    /// 404: any path other than [`LINK_PATH`], query strings included.
    NotFound,
    /// 426: a `Sec-WebSocket-Version` other than 13 (or none).
    UpgradeRequired,
    /// 431: no end of headers within [`MAX_REQUEST`] bytes.
    TooLarge,
}

impl Refusal {
    /// The HTTP status code.
    pub const fn status(self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::NotFound => 404,
            Self::UpgradeRequired => 426,
            Self::TooLarge => 431,
        }
    }

    /// The whole response to write before closing.
    pub const fn response(self) -> &'static [u8] {
        match self {
            Self::BadRequest => {
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            }
            Self::NotFound => {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            }
            Self::UpgradeRequired => {
                b"HTTP/1.1 426 Upgrade Required\r\nSec-WebSocket-Version: 13\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            }
            Self::TooLarge => {
                b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            }
        }
    }
}

const UPGRADE_HEAD: &[u8] = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ";

/// The length of every [`upgrade_response`].
pub const UPGRADE_RESPONSE_LEN: usize = UPGRADE_HEAD.len() + 28 + 4;

/// The `101 Switching Protocols` response for a request's key.
pub fn upgrade_response(key: &str) -> [u8; UPGRADE_RESPONSE_LEN] {
    let mut out = [0u8; UPGRADE_RESPONSE_LEN];
    let (head, rest) = out.split_at_mut(UPGRADE_HEAD.len());
    head.copy_from_slice(UPGRADE_HEAD);
    rest[..28].copy_from_slice(&accept_key(key));
    rest[28..].copy_from_slice(b"\r\n\r\n");
    out
}

/// Read the request at the start of `buf`. Call it again with the grown
/// buffer while it answers [`Handshake::Incomplete`]; a request split over
/// many reads parses the same as one that arrived whole.
pub fn parse_request(buf: &[u8]) -> Handshake<'_> {
    let Some(end) = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
    else {
        return if buf.len() >= MAX_REQUEST {
            Handshake::Refuse {
                refusal: Refusal::TooLarge,
                consumed: buf.len(),
            }
        } else {
            Handshake::Incomplete
        };
    };
    if end > MAX_REQUEST {
        return Handshake::Refuse {
            refusal: Refusal::TooLarge,
            consumed: end,
        };
    }
    match check_request(&buf[..end - 4]) {
        Ok(key) => Handshake::Upgrade { key, consumed: end },
        Err(refusal) => Handshake::Refuse {
            refusal,
            consumed: end,
        },
    }
}

/// Optional whitespace around header values and list items (RFC 9110, 5.6.3).
const OWS: [char; 2] = [' ', '\t'];

/// Validate the request line and headers (no terminator); the key on success.
fn check_request(head: &[u8]) -> Result<&str, Refusal> {
    let head = core::str::from_utf8(head).map_err(|_| Refusal::BadRequest)?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (method, target, version) = (
        request_line.next(),
        request_line.next(),
        request_line.next(),
    );
    if method != Some("GET") || version != Some("HTTP/1.1") || request_line.next().is_some() {
        return Err(Refusal::BadRequest);
    }
    if target != Some(LINK_PATH) {
        return Err(Refusal::NotFound);
    }

    let (mut upgrade, mut connection, mut version_13) = (false, false, false);
    let mut key = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(Refusal::BadRequest)?;
        if name.is_empty() || name.contains(OWS) {
            return Err(Refusal::BadRequest);
        }
        let value = value.trim_matches(OWS);
        if name.eq_ignore_ascii_case("upgrade") {
            upgrade |= has_token(value, "websocket");
        } else if name.eq_ignore_ascii_case("connection") {
            connection |= has_token(value, "upgrade");
        } else if name.eq_ignore_ascii_case("sec-websocket-version") {
            version_13 = value == "13";
        } else if name.eq_ignore_ascii_case("sec-websocket-key") {
            if key.replace(value).is_some() {
                return Err(Refusal::BadRequest);
            }
        }
    }
    if !upgrade || !connection {
        return Err(Refusal::BadRequest);
    }
    if !version_13 {
        return Err(Refusal::UpgradeRequired);
    }
    key.filter(|k| is_valid_key(k)).ok_or(Refusal::BadRequest)
}

/// Does the comma-separated `list` contain `token` (case-insensitive)?
fn has_token(list: &str, token: &str) -> bool {
    list.split(',')
        .any(|item| item.trim_matches(OWS).eq_ignore_ascii_case(token))
}

/// A key is base64 of 16 bytes: 22 alphabet characters and `==`.
fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    bytes.len() == 24
        && bytes.ends_with(b"==")
        && bytes[..22]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/')
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::string::String;

    use super::*;

    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    #[test]
    fn accepts_a_browser_request() {
        let req = request(
            "/link",
            &[
                "Host: 192.168.1.40",
                "Connection: keep-alive, Upgrade",
                "Upgrade: websocket",
                "Origin: https://lightplayer.app",
                "Sec-WebSocket-Version: 13",
                "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
                "Sec-WebSocket-Protocol: ignored",
            ],
        );
        assert_eq!(
            parse_request(req.as_bytes()),
            Handshake::Upgrade {
                key: KEY,
                consumed: req.len()
            }
        );
    }

    #[test]
    fn headers_are_case_insensitive() {
        let req = request(
            "/link",
            &[
                "UPGRADE: WebSocket",
                "connection: UPGRADE",
                "sec-websocket-version:13",
                "SEC-WEBSOCKET-KEY:   dGhlIHNhbXBsZSBub25jZQ==  ",
            ],
        );
        assert!(matches!(
            parse_request(req.as_bytes()),
            Handshake::Upgrade { key: KEY, .. }
        ));
    }

    #[test]
    fn incomplete_until_the_blank_line_then_keeps_trailing_bytes() {
        let req = request("/link", &valid_headers());
        for cut in 0..req.len() {
            assert_eq!(
                parse_request(&req.as_bytes()[..cut]),
                Handshake::Incomplete,
                "cut {cut}"
            );
        }
        let mut with_frame = String::from(&req);
        with_frame.push_str("\u{82}");
        assert_eq!(
            parse_request(with_frame.as_bytes()),
            Handshake::Upgrade {
                key: KEY,
                consumed: req.len()
            }
        );
    }

    #[test]
    fn refusals() {
        let refusal = |req: &str| match parse_request(req.as_bytes()) {
            Handshake::Refuse { refusal, .. } => Some(refusal),
            _ => None,
        };
        let h = valid_headers();
        assert_eq!(refusal(&request("/", &h)), Some(Refusal::NotFound));
        assert_eq!(refusal(&request("/link?x=1", &h)), Some(Refusal::NotFound));
        assert_eq!(refusal(&request("/link/", &h)), Some(Refusal::NotFound));
        assert_eq!(
            refusal(&request("/link", &without(&h, "Sec-WebSocket-Key"))),
            Some(Refusal::BadRequest)
        );
        assert_eq!(
            refusal(&request("/link", &without(&h, "Upgrade:"))),
            Some(Refusal::BadRequest)
        );
        assert_eq!(
            refusal(&request("/link", &without(&h, "Connection"))),
            Some(Refusal::BadRequest)
        );
        assert_eq!(
            refusal(&request("/link", &without(&h, "Sec-WebSocket-Version"))),
            Some(Refusal::UpgradeRequired)
        );
        let mut v8 = without(&h, "Sec-WebSocket-Version");
        v8.push("Sec-WebSocket-Version: 8");
        assert_eq!(
            refusal(&request("/link", &v8)),
            Some(Refusal::UpgradeRequired)
        );
        let mut short_key = without(&h, "Sec-WebSocket-Key");
        short_key.push("Sec-WebSocket-Key: abc==");
        assert_eq!(
            refusal(&request("/link", &short_key)),
            Some(Refusal::BadRequest)
        );
        let mut two_keys = h.clone();
        two_keys.push("Sec-WebSocket-Key: AQIDBAUGBwgJCgsMDQ4PEA==");
        assert_eq!(
            refusal(&request("/link", &two_keys)),
            Some(Refusal::BadRequest)
        );
        let post = request("/link", &h).replacen("GET", "POST", 1);
        assert_eq!(refusal(&post), Some(Refusal::BadRequest));
        let http10 = request("/link", &h).replacen("HTTP/1.1", "HTTP/1.0", 1);
        assert_eq!(refusal(&http10), Some(Refusal::BadRequest));
        let folded =
            request("/link", &h).replacen("Upgrade: websocket", "Upgrade:\r\n websocket", 1);
        assert_eq!(refusal(&folded), Some(Refusal::BadRequest));
    }

    #[test]
    fn oversize_request_is_refused_with_431() {
        let filler = "a".repeat(MAX_REQUEST);
        let unterminated = std::format!("GET /link HTTP/1.1\r\nX: {filler}");
        assert!(matches!(
            parse_request(unterminated.as_bytes()),
            Handshake::Refuse {
                refusal: Refusal::TooLarge,
                ..
            }
        ));
        let mut long: std::vec::Vec<&str> = valid_headers();
        let header = std::format!("X: {filler}");
        long.push(&header);
        assert!(matches!(
            parse_request(request("/link", &long).as_bytes()),
            Handshake::Refuse {
                refusal: Refusal::TooLarge,
                ..
            }
        ));
        assert_eq!(Refusal::TooLarge.status(), 431);
    }

    #[test]
    fn upgrade_response_carries_the_accept_key() {
        let resp = upgrade_response(KEY);
        let text = core::str::from_utf8(&resp).unwrap();
        assert_eq!(
            text,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n"
        );
    }

    #[test]
    fn refusal_responses_are_complete_and_closing() {
        for r in [
            Refusal::BadRequest,
            Refusal::NotFound,
            Refusal::UpgradeRequired,
            Refusal::TooLarge,
        ] {
            let text = core::str::from_utf8(r.response()).unwrap();
            assert!(
                text.starts_with(&std::format!("HTTP/1.1 {} ", r.status())),
                "{text}"
            );
            assert!(text.contains("\r\nContent-Length: 0\r\n"), "{text}");
            assert!(text.contains("\r\nConnection: close\r\n"), "{text}");
            assert!(text.ends_with("\r\n\r\n"), "{text}");
        }
        assert!(
            core::str::from_utf8(Refusal::UpgradeRequired.response())
                .unwrap()
                .contains("\r\nSec-WebSocket-Version: 13\r\n")
        );
    }

    fn valid_headers() -> std::vec::Vec<&'static str> {
        std::vec![
            "Host: board",
            "Upgrade: websocket",
            "Connection: Upgrade",
            "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
            "Sec-WebSocket-Version: 13",
        ]
    }

    fn without<'a>(headers: &[&'a str], prefix: &str) -> std::vec::Vec<&'a str> {
        headers
            .iter()
            .copied()
            .filter(|h| !h.starts_with(prefix))
            .collect()
    }

    fn request(path: &str, headers: &[&str]) -> String {
        let mut req = std::format!("GET {path} HTTP/1.1\r\n");
        for h in headers {
            req.push_str(h);
            req.push_str("\r\n");
        }
        req.push_str("\r\n");
        req
    }
}
