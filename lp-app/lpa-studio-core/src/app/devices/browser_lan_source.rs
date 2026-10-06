//! [`LanLinkSource`] over the page's WebSockets (wasm only): what a board on
//! the LAN actually is in the browser.
//!
//! The thinnest join, like `browser_ble_source.rs`: `lpa-link`'s
//! `browser_websocket` owns the sockets, the bounded connect and the
//! reconnect loop; `BrowserWebsocketLink` turns a session into the model's
//! link; `WsClientIo` is the borrowing conversation's io. What is left here
//! is opening one session per `?lan=` address, handing the page's link keys
//! to the provider, and keeping one wire handle per session for the link and
//! a borrowing conversation to share.
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The transport it plugs
//! into is host-covered through `lan_transport.rs`'s double; the provider
//! below it is pinned by `lpa-link/tests/browser_websocket_conformance.rs`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_link::device_link::browser_websocket::{BrowserWebsocketLink, lan_link_info};
use lpa_link::providers::browser_websocket::{
    self as lan, LanSession, WsClientIo, WsTapLine, WsWire,
};
use lpa_link::providers::network_link::LinkKeys;

use super::device_transport::{DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent};
use super::lan_transport::LanLinkSource;

/// LAN boards, as this page holds them.
pub struct BrowserLanSource {
    /// One wire per JS session, by the board's socket URL.
    wires: Rc<RefCell<BTreeMap<String, Rc<WsWire>>>>,
}

impl BrowserLanSource {
    /// Reach the boards at `urls` (the `?lan=` flag's, normalised), each
    /// link presenting `keys` (the access layer's). Every session connects
    /// now and keeps reconnecting; each board is present once it answers.
    pub fn new(urls: &[String], keys: Rc<dyn LinkKeys>) -> Self {
        lan::set_link_keys(keys);
        for url in urls {
            if let Err(error) = lan::open_session(url) {
                log::warn!("wi-fi: {url} not opened: {error}");
            }
        }
        Self {
            wires: Rc::default(),
        }
    }

    fn granted(
        wires: &Rc<RefCell<BTreeMap<String, Rc<WsWire>>>>,
        session: &LanSession,
    ) -> GrantedLink {
        let wire = {
            let mut wires = wires.borrow_mut();
            let wire = wires
                .entry(session.url.clone())
                .or_insert_with(|| Rc::new(WsWire::new(session.session)));
            if wire.session() != session.session {
                *wire = Rc::new(WsWire::new(session.session));
            }
            Rc::clone(wire)
        };
        let info = lan_link_info(session);
        GrantedLink {
            link: Box::new(BrowserWebsocketLink::new(wire, info.clone())),
            info,
        }
    }
}

impl LanLinkSource for BrowserLanSource {
    fn present(&self) -> Vec<GrantedLink> {
        lan::present_sessions()
            .iter()
            .map(|session| Self::granted(&self.wires, session))
            .collect()
    }

    fn forget(&self, url: &str) -> DeviceTransportFuture<Result<(), String>> {
        let wire = self.wires.borrow_mut().remove(url);
        Box::pin(async move {
            if let Some(wire) = wire {
                lan::forget(wire.session()).await;
            }
            Ok(())
        })
    }

    fn client_io(
        &self,
        url: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let wire = self
            .wires
            .borrow()
            .get(url)
            .cloned()
            .ok_or_else(|| "this Wi-Fi board is not connected".to_string())?;
        let tap: Option<Rc<dyn Fn(WsTapLine)>> = tap.map(|tap| {
            Rc::new(move |line: WsTapLine| {
                tap(match line {
                    WsTapLine::Line(line) => LensTapEvent::Line(line),
                    // A reset among the notes fails the shared conversations
                    // (D9), as over Web Serial and Bluetooth.
                    WsTapLine::Note(note) => LensTapEvent::Note(note),
                    WsTapLine::PortError(error) => LensTapEvent::PortError(error),
                })
            }) as Rc<dyn Fn(WsTapLine)>
        });
        Ok(Box::new(WsClientIo::new(wire, tap)))
    }
}
