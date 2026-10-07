//! A board through the cloud relay: `relay:<board>[@<origin>]` (feature
//! `lan`; Wi-Fi roadmap M7).
//!
//! The relay's browser leg carries bare lp-link frames, one per binary
//! WebSocket message — exactly what a board serves on its LAN `/link`. So a
//! `relay:` link is a [`crate::transport_lan`] link with a different
//! [`LinkEndpoint`](crate::transport_lan::LinkEndpoint): the socket goes to
//! `wss://<origin>/relay/board/<id>` and presents the session cookie, and
//! the same secure lp-link, key walk and pump run above it
//! ([`crate::transport_lan::connect_lan_transport`]).
//!
//! Through the relay the board never grants its "Anyone" tier, so a link
//! comes up keyed by a held account key ([`LanOptions::held_keys`]) or by
//! the board's password, never anonymously.
//!
//! [`LanOptions::held_keys`]: crate::transport_lan::LanOptions::held_keys

mod relay_target;

pub use relay_target::{DEFAULT_RELAY_ORIGIN, RELAY_SESSION_COOKIE, RelayTarget};
