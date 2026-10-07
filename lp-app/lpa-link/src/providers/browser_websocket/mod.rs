//! A board on the LAN over a browser WebSocket: the board's wire on a
//! SECURE lp-link (Wi-Fi M6 P07, the LAN half of the `browser-websocket`
//! provider).
//!
//! The same provider reaches a board THROUGH lightplayer.app's relay (the
//! network transport's P05): the relay's browser leg
//! (`wss://<host>/relay/board/<mac>`, [`open_relay_session`]) carries the
//! same frames, so the session, the link and the conversation below are the
//! LAN's. Two things differ, both decided from the socket URL: the link
//! presents only the keys the page holds — no anonymous key
//! ([`KeyWalk::held_only`](crate::providers::network_link::KeyWalk::held_only))
//! — and gives the session up when none opens the board; and the session's
//! words start `relay …`.
//!
//! A C6 on Wi-Fi serves `ws://<board>/link` (and `lp-xxxx.local`); each
//! binary message is one lp-link frame, and the board is a Noise NNpsk0
//! responder. This provider follows the Web Bluetooth adapter's shape: the
//! JS module owns the socket and its connection, a thin Rust binding
//! reaches it through `#[wasm_bindgen(module = …)]` with no `web-sys`
//! WebSocket features, and each connection runs the SAME `LinkPortService`
//! every browser port runs — built secure, on [`LinkConfig::ws`]'s
//! datagrams, presenting the app's keys ([`set_link_keys`]).
//!
//! | file | what it owns |
//! |---|---|
//! | `browser_websocket.js` | the `WebSocket`, the bounded connect, one message per frame, the reconnect loop, presence edges |
//! | `browser_websocket.rs` | the bindings and the session descriptor ([`LanSession`]) |
//! | `ws_link_keys.rs` | the page's key source ([`set_link_keys`]) |
//! | `ws_link_port.rs` | each connection's secure lp-link end, its key walk, and the loop that services it |
//! | `ws_wire.rs` | [`WsWire`] — a handle on a session's wire, shared by the link and a borrowing conversation |
//! | `ws_client_io.rs` | [`WsClientIo`] — `lpa-client`'s io over the wire, for push/remove/manifest writes and the editor lens |
//!
//! The model's `Link` over this lives in `device_link::browser_websocket`;
//! the endpoint, the keys and the key walk are the host-tested
//! [`network_link`](crate::providers::network_link).
//!
//! What a LAN link cannot do is stated once, in
//! [`LinkProviderKind::BrowserWebsocket`](crate::LinkProviderKind::BrowserWebsocket)'s
//! capabilities: no reset, no flash, no erase, no boot control, no raw
//! filesystem.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The browser half is pinned
//! by `tests/browser_websocket_conformance.rs` (`just lpa-link-browser-test`).
//!
//! [`LinkConfig::ws`]: lpc_wire::lp_link::LinkConfig::ws

mod browser_websocket;
mod ws_client_io;
mod ws_link_keys;
mod ws_link_port;
mod ws_wire;

pub use browser_websocket::{
    LanSession, connect_and_settle, connect_until_up, forget, install_websocket_events,
    is_supported, open_relay_session, open_session, present_sessions,
};
pub use ws_client_io::{WsClientIo, WsTapLine};
pub use ws_link_keys::set_link_keys;
pub use ws_link_port::{PLAIN_LINK_NOTE, RELAY_NO_HELD_KEY};
pub use ws_wire::{WsWire, is_link_lost};
