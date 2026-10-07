//! Where a network link's WebSocket goes: a full URL, and what to present.
//!
//! One secure lp-link inside one WebSocket is the same thing whether the
//! socket goes to a board on the LAN (`ws://<board>/link`, a
//! [`super::LanTarget`]) or to a board through the cloud relay
//! (`wss://lightplayer.app/relay/board/<id>`, a
//! [`crate::transport_relay::RelayTarget`]). What differs is only where
//! the socket goes, the session cookie the relay wants, and which close
//! code means "busy, try again" — all of it here.

use std::fmt;

/// See the module doc.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkEndpoint {
    url: String,
    /// The `Cookie` header's value, if the endpoint wants one. A session
    /// token: never printed.
    cookie: Option<String>,
    label: String,
    busy_codes: &'static [u16],
}

/// The close code a board on the LAN sends when every LAN slot it has is
/// taken: 1013, "try again later".
pub const LAN_BUSY_CLOSE: u16 = 1013;

impl LinkEndpoint {
    /// A socket to `url` (`ws://` or `wss://`), called `label` in messages,
    /// for which a close with any of `busy_codes` means the far end has no
    /// room right now.
    pub fn new(
        url: impl Into<String>,
        label: impl Into<String>,
        busy_codes: &'static [u16],
    ) -> Self {
        Self {
            url: url.into(),
            cookie: None,
            label: label.into(),
            busy_codes,
        }
    }

    /// The same endpoint, presenting `cookie` (a `Cookie` header value such
    /// as `lp_session=…`) on the upgrade.
    #[must_use]
    pub fn with_cookie(mut self, cookie: Option<String>) -> Self {
        self.cookie = cookie;
        self
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn cookie(&self) -> Option<&str> {
        self.cookie.as_deref()
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether a close with `code` means "busy, try again".
    pub fn is_busy(&self, code: u16) -> bool {
        self.busy_codes.contains(&code)
    }
}

/// The endpoint's label (`lan:lp-3f2a.local`, `relay:10bda3b08e30`).
impl fmt::Display for LinkEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// The cookie is a credential: only whether there is one.
impl fmt::Debug for LinkEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkEndpoint")
            .field("url", &self.url)
            .field("cookie", &self.cookie.as_ref().map(|_| "<set>"))
            .field("label", &self.label)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_cookie_and_busy_is_per_endpoint() {
        let endpoint = LinkEndpoint::new("wss://x/relay/board/1", "relay:1", &[4429])
            .with_cookie(Some("lp_session=secret".into()));
        assert!(!format!("{endpoint:?}").contains("secret"));
        assert_eq!(endpoint.to_string(), "relay:1");
        assert!(endpoint.is_busy(4429));
        assert!(!endpoint.is_busy(LAN_BUSY_CLOSE));
    }
}
