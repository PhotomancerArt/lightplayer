//! [`RelayLinkSource`] over the page's WebSockets (wasm only): what a board
//! reached through lightplayer.app's relay actually is in the browser.
//!
//! The LAN source's twin (`browser_lan_source.rs`) over the SAME provider
//! (`lpa-link`'s `browser_websocket`): each board is one session to the
//! relay's browser leg on this page's own origin,
//! `wss://<host>/relay/board/<mac>` — the cookie that says who is signed in
//! rides along because it is the same origin. What is left here is opening a
//! session per board Studio was asked to reach (the `?relay=<mac>` dev
//! shortcut at page load; a card's Connect later, through
//! [`RelayLinkSource::connect`]), naming the relay's refusals that end a
//! session rather than redial it, and keeping one wire handle per session.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The transport it plugs
//! into is host-covered through `relay_transport.rs`'s double; the provider
//! below it is pinned by `lpa-link/tests/browser_websocket_conformance.rs`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_link::device_link::browser_websocket::{BrowserWebsocketLink, network_link_info};
use lpa_link::providers::browser_websocket::{
    self as ws, LanSession, WsClientIo, WsTapLine, WsWire,
};
use lpa_link::providers::network_link::{LinkKeys, board_from_relay_socket_url, relay_socket_url};
use lpc_relay::RelayCloseCode;

use super::device_transport::{DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent};
use super::relay_transport::RelayLinkSource;

/// How long a connect someone asked for waits for the board to accept a key
/// once the relay's socket is up, before it calls the board reached anyway.
/// Longer than the LAN's: every key the walk presents is a round trip
/// through the relay.
const SETTLE_MS: u32 = 8_000;

/// The relay's refusals that a redial would only meet again — and each
/// redial spends this page's tries at the relay (its per-address limit,
/// `4420`). A session closed with one of them ends.
const FINAL_CODES: [RelayCloseCode; 4] = [
    RelayCloseCode::SignInRequired,
    RelayCloseCode::BoardOffline,
    RelayCloseCode::SlowDown,
    RelayCloseCode::Busy,
];

/// The refusals an update rides through, and how long it waits before each
/// redial (`ws::hold`; the link holds its session for a while after every
/// update message it sends). A board resets three times in an update, and
/// until it is back the relay says it is offline — and each redial at a
/// board that is not online spends one of this page's tries (20, one more
/// every 30 s): so every 3 s while it is offline, and when the tries run
/// out ("slow down") the 30 s one try takes to come back.
const HOLD_CODES: [(RelayCloseCode, u32); 2] = [
    (RelayCloseCode::BoardOffline, 3_000),
    (RelayCloseCode::SlowDown, 30_000),
];

/// Relay boards, as this page holds them.
pub struct BrowserRelaySource {
    /// The relay's origin: this page's own (`https://lightplayer.app`, or a
    /// dev server that forwards `/relay` to a local `lp-cloud-server`).
    origin: String,
    /// One wire per JS session, by board.
    wires: Rc<RefCell<BTreeMap<String, Rc<WsWire>>>>,
}

impl BrowserRelaySource {
    /// Reach `boards` (the `?relay=<mac>` shortcut's; often none) through
    /// the relay at `origin`, each link presenting `keys` (the access
    /// layer's: held keys only, through the relay).
    pub fn new(boards: &[String], origin: &str, keys: Rc<dyn LinkKeys>) -> Self {
        ws::set_link_keys(keys);
        let source = Self {
            origin: origin.trim_end_matches('/').to_string(),
            wires: Rc::default(),
        };
        for board in boards {
            if let Err(error) = source.open(board) {
                log::warn!("relay: {board} not opened: {error}");
            }
        }
        source
    }

    fn open(&self, board: &str) -> Result<LanSession, String> {
        let codes: Vec<u16> = FINAL_CODES.iter().map(|code| code.code()).collect();
        let held: Vec<(u16, u32)> = HOLD_CODES
            .iter()
            .map(|(code, delay_ms)| (code.code(), *delay_ms))
            .collect();
        ws::open_relay_session(&relay_socket_url(&self.origin, board), &codes, &held)
    }

    fn granted(
        wires: &Rc<RefCell<BTreeMap<String, Rc<WsWire>>>>,
        board: &str,
        session: &LanSession,
    ) -> GrantedLink {
        let wire = {
            let mut wires = wires.borrow_mut();
            let wire = wires
                .entry(board.to_string())
                .or_insert_with(|| Rc::new(WsWire::new(session.session)));
            if wire.session() != session.session {
                *wire = Rc::new(WsWire::new(session.session));
            }
            Rc::clone(wire)
        };
        let info = network_link_info(session);
        GrantedLink {
            link: Box::new(BrowserWebsocketLink::new(wire, info.clone())),
            info,
        }
    }
}

impl RelayLinkSource for BrowserRelaySource {
    fn present(&self) -> Vec<GrantedLink> {
        ws::present_sessions()
            .iter()
            .filter_map(|session| {
                let board = board_from_relay_socket_url(&session.url)?;
                Some(Self::granted(&self.wires, board, session))
            })
            .collect()
    }

    fn forget(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
        let wire = self.wires.borrow_mut().remove(board);
        Box::pin(async move {
            if let Some(wire) = wire {
                ws::forget(wire.session()).await;
            }
            Ok(())
        })
    }

    fn client_io(
        &self,
        board: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let wire = self
            .wires
            .borrow()
            .get(board)
            .cloned()
            .ok_or_else(|| "this board is not connected through lightplayer.app".to_string())?;
        let tap: Option<Rc<dyn Fn(WsTapLine)>> = tap.map(|tap| {
            Rc::new(move |line: WsTapLine| {
                tap(match line {
                    WsTapLine::Line(line) => LensTapEvent::Line(line),
                    WsTapLine::Note(note) => LensTapEvent::Note(note),
                    WsTapLine::PortError(error) => LensTapEvent::PortError(error),
                })
            }) as Rc<dyn Fn(WsTapLine)>
        });
        Ok(Box::new(WsClientIo::new(wire, tap)))
    }

    fn connect(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
        let opened = self.open(board);
        let board = board.to_string();
        Box::pin(async move {
            let session = opened?;
            let reached = ws::connect_until_up(session.session, SETTLE_MS).await;
            if let Err(error) = &reached {
                // Nothing keeps redialling a board the relay turned away:
                // the person who asked hears why, and asks again.
                log::info!("relay: {board}: {error}");
                ws::forget(session.session).await;
            }
            reached
        })
    }
}
