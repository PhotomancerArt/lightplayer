//! Where a board listens on the LAN: `lan:<host>[:port]`.

use std::fmt;

pub use crate::specifier::LAN_DEFAULT_PORT;

/// The one WebSocket route a board serves its link on.
pub const LAN_LINK_PATH: &str = "/link";

/// A board on the LAN: an IPv4 address or a `.local` name (resolved by the
/// OS resolver, which on macOS speaks mDNS), and its link port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanTarget {
    pub host: String,
    pub port: u16,
}

impl LanTarget {
    /// The board a `lan:` host specifier names (`None` for any other kind).
    pub fn from_specifier(spec: &crate::HostSpecifier) -> Option<Self> {
        match spec {
            crate::HostSpecifier::Lan { host, port } => Some(Self::new(host.clone(), *port)),
            _ => None,
        }
    }

    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// The board's link endpoint: `ws://<host>:<port>/link`.
    pub fn url(&self) -> String {
        format!("ws://{}:{}{LAN_LINK_PATH}", self.host, self.port)
    }
}

impl fmt::Display for LanTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.port == LAN_DEFAULT_PORT {
            write!(f, "lan:{}", self.host)
        } else {
            write!(f, "lan:{}:{}", self.host, self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_the_link_route_and_display_round_trips_the_address() {
        let board = LanTarget::new("lp-3f2a.local", LAN_DEFAULT_PORT);
        assert_eq!(board.url(), "ws://lp-3f2a.local:80/link");
        assert_eq!(board.to_string(), "lan:lp-3f2a.local");
        let harness = LanTarget::new("127.0.0.1", 50123);
        assert_eq!(harness.to_string(), "lan:127.0.0.1:50123");
    }
}
