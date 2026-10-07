//! A board through the cloud relay: `relay:<board>[@<origin>]`.

use std::fmt;

use lpc_relay::{RelayBoardId, RelayCloseCode};

use crate::transport_lan::LinkEndpoint;

pub use crate::specifier::RELAY_DEFAULT_ORIGIN as DEFAULT_RELAY_ORIGIN;

/// The cookie the relay reads a browser session from.
pub const RELAY_SESSION_COOKIE: &str = "lp_session";

/// A board's id at the relay and the relay's origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayTarget {
    pub board: RelayBoardId,
    /// `https://lightplayer.app`, or `http://127.0.0.1:<port>` for a local
    /// `lp-cloud-server`.
    pub origin: String,
}

impl RelayTarget {
    /// The board a `relay:` host specifier names (`None` for any other kind).
    pub fn from_specifier(spec: &crate::HostSpecifier) -> Option<Self> {
        match spec {
            crate::HostSpecifier::Relay { board, origin } => Some(Self {
                board: *board,
                origin: origin.clone(),
            }),
            _ => None,
        }
    }

    /// A board `board` on the relay at `origin`.
    pub fn new(board: RelayBoardId, origin: impl Into<String>) -> Self {
        Self {
            board,
            origin: origin.into(),
        }
    }

    /// The browser leg's URL: `wss://<host>/relay/board/<id>` (`ws://` for
    /// an `http://` origin).
    pub fn url(&self) -> String {
        let socket_origin = if let Some(rest) = self.origin.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.origin.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.origin.clone()
        };
        format!(
            "{}/relay/board/{}",
            socket_origin.trim_end_matches('/'),
            self.board
        )
    }

    /// Where the link's socket goes, presenting `session` (the raw value of
    /// the `lp_session` cookie: `LP_CLOUD_SESSION`, never argv) if given.
    pub fn endpoint(&self, session: Option<&str>) -> LinkEndpoint {
        const BUSY: &[u16] = &[RelayCloseCode::Busy.code()];
        LinkEndpoint::new(self.url(), self.to_string(), BUSY)
            .with_cookie(session.map(|token| format!("{RELAY_SESSION_COOKIE}={token}")))
    }
}

/// `relay:10bda3b08e30`, with `@<origin>` when it is not the default.
impl fmt::Display for RelayTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.origin == DEFAULT_RELAY_ORIGIN {
            write!(f, "relay:{}", self.board)
        } else {
            write!(f, "relay:{}@{}", self.board, self.origin)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_the_browser_leg_on_the_origins_socket_scheme() {
        let board = RelayBoardId([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30]);
        let prod = RelayTarget {
            board,
            origin: DEFAULT_RELAY_ORIGIN.into(),
        };
        assert_eq!(prod.url(), "wss://lightplayer.app/relay/board/10bda3b08e30");
        assert_eq!(prod.to_string(), "relay:10bda3b08e30");
        let dev = RelayTarget {
            board,
            origin: "http://127.0.0.1:2812/".into(),
        };
        assert_eq!(dev.url(), "ws://127.0.0.1:2812/relay/board/10bda3b08e30");
        assert_eq!(dev.to_string(), "relay:10bda3b08e30@http://127.0.0.1:2812/");
    }

    #[test]
    fn the_session_rides_as_the_cookie_and_busy_is_4429() {
        let target = RelayTarget {
            board: RelayBoardId([1; 6]),
            origin: DEFAULT_RELAY_ORIGIN.into(),
        };
        let endpoint = target.endpoint(Some("tok"));
        assert_eq!(endpoint.cookie(), Some("lp_session=tok"));
        assert!(endpoint.is_busy(4429));
        assert_eq!(target.endpoint(None).cookie(), None);
    }
}
