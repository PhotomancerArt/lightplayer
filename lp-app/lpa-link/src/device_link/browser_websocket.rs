//! [`Link`] over a LAN session — a browser WebSocket to a board on Wi-Fi
//! (wasm only) — or over a relay session, the same socket code reaching a
//! board through lightplayer.app (the network transport's P05). One adapter:
//! the endpoint (`lan:` or `relay:`) and the first word of what it says
//! (`wi-fi …` or `relay …`) are all that differ.
//!
//! The Wi-Fi twin of `browser_ble`: the JS module owns the socket, this
//! adapter turns that promise-shaped surface into the model's event-queue
//! contract. Connect and close are awaited in a spawned future, drained one
//! command at a time so a `Close` cannot overtake the `Open` it follows
//! (invariant I7: the fold never awaits device IO).
//!
//! - **The link is usually already up when the model opens it.** A `?lan=`
//!   board is connected by the page's session as soon as the flag is read,
//!   so `Open` is normally just "start listening"; only a link the model
//!   closed earlier pays for a (bounded) connect.
//! - **There is no reset.** A `RunReset` fails by name.
//! - **A drop is a departure.** The JS reports it as `wi-fi link lost: …`,
//!   which the effects layer's pump reads as the link being gone; the
//!   session's reconnect loop announces the board again through the presence
//!   edge, and the sweep re-attaches it. `Open` discards a loss recorded
//!   before it (the Bluetooth link's lesson,
//!   `2026-10-06-a-bluetooth-reconnect-reads-the-old-links-loss`).
//! - **The wire is a secure lp-link.** Each connection is an lp-link on
//!   `LinkConfig::ws()`'s datagrams with the app's keys, serviced by the
//!   provider (`providers/browser_websocket/ws_link_port.rs`). A request is
//!   one link message; what the board said comes back as
//!   [`WireRead`](crate::device_link::wire_reader::WireRead)s, demuxed as
//!   every port's are; the link's own notes — a refused key among them — and
//!   a reset within a connection (a rekey) reach the journal as
//!   `LinkEvent::WireNote`s.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo};
use wasm_bindgen_futures::spawn_local;

use crate::device_link::demux::demux_read;
use crate::device_link::wire::client_message;
use crate::providers::browser_websocket::{LanSession, WsWire, is_link_lost};

/// The [`LinkInfo`] a LAN session's link wears (see
/// [`network_link::lan_link_info`](crate::providers::network_link::lan_link_info)).
pub fn lan_link_info(session: &LanSession) -> LinkInfo {
    crate::providers::network_link::lan_link_info(&session.url)
}

/// The [`LinkInfo`] a session's link wears, whichever kind it is: `relay:`
/// for the relay's browser leg, `lan:` for anything else.
pub fn network_link_info(session: &LanSession) -> LinkInfo {
    crate::providers::network_link::relay_link_info(&session.url)
        .unwrap_or_else(|| lan_link_info(session))
}

/// One [`Link`] over a LAN session.
pub struct BrowserWebsocketLink {
    inner: Rc<WsLinkInner>,
}

impl BrowserWebsocketLink {
    /// Wrap a session's wire. Nothing is read until the model opens it.
    pub fn new(wire: Rc<WsWire>, info: LinkInfo) -> Self {
        Self {
            inner: Rc::new(WsLinkInner {
                wire,
                info,
                events: RefCell::new(VecDeque::new()),
                queue: RefCell::new(VecDeque::new()),
                open: Cell::new(false),
                draining: Cell::new(false),
            }),
        }
    }

    /// Whether the model has this link open.
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }
}

impl Link for BrowserWebsocketLink {
    fn info(&self) -> &LinkInfo {
        &self.inner.info
    }

    fn submit(&mut self, command: LinkCommand) {
        // Every command waits in one queue, so a write never jumps an `Open`
        // whose connect is still in flight.
        self.inner.queue.borrow_mut().push_back(command);
        WsLinkInner::drain(&self.inner);
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.inner.events.borrow().is_empty() {
            self.inner.pump();
        }
        self.inner.events.borrow_mut().pop_front()
    }
}

struct WsLinkInner {
    wire: Rc<WsWire>,
    info: LinkInfo,
    events: RefCell<VecDeque<LinkEvent>>,
    queue: RefCell<VecDeque<LinkCommand>>,
    open: Cell<bool>,
    draining: Cell<bool>,
}

impl WsLinkInner {
    fn drain(inner: &Rc<Self>) {
        if inner.draining.get() {
            return;
        }
        inner.draining.set(true);
        let inner = Rc::clone(inner);
        spawn_local(async move {
            loop {
                let next = inner.queue.borrow_mut().pop_front();
                let Some(command) = next else {
                    break;
                };
                inner.execute(command).await;
            }
            inner.draining.set(false);
        });
    }

    async fn execute(&self, command: LinkCommand) {
        match command {
            LinkCommand::Open { .. } => self.open_link().await,
            LinkCommand::Close => self.close_link("closed by request").await,
            LinkCommand::RunReset(kind) => {
                self.push(LinkEvent::Error(format!(
                    "a {} link has no reset lines; reset needs USB",
                    if self.info.endpoint.is_relay() {
                        "relay"
                    } else {
                        "Wi-Fi"
                    }
                )));
                self.push(LinkEvent::ResetOutcome { kind, ok: false });
            }
            LinkCommand::SendFrame(frame) => match client_message(&frame) {
                Ok(message) => match lpc_wire::json::to_string(&message) {
                    Ok(json) => self.send_json(&json),
                    Err(error) => self.push(LinkEvent::Error(format!(
                        "failed to encode {:?}: {error}",
                        frame.body
                    ))),
                },
                Err(error) => self.push(LinkEvent::Error(error)),
            },
            // A conversation's request, still spelled as the line it was
            // (`M!{json}`): on the link it is one message.
            LinkCommand::SendLine(line) => match line.trim_end().strip_prefix("M!") {
                Some(json) => self.send_json(json),
                None => self.push(LinkEvent::Error(format!(
                    "not a request, and the link carries no raw text to the board: {line:?}"
                ))),
            },
            // No update channel on a LAN link yet, and its `LinkInfo` says
            // so: the model never asks. Dropped.
            LinkCommand::SendUpdate(_) => {}
        }
    }

    /// Start listening, connecting first if the socket is not open.
    async fn open_link(&self) {
        if !self.wire.is_connected()
            && let Err(error) = self.wire.connect().await
        {
            self.push(LinkEvent::Error(format!(
                "{} connect failed: {error}",
                self.kind()
            )));
            return;
        }
        // A `wi-fi link lost` still queued here was an earlier link's; read
        // now, it would close this link the moment it opened.
        if let Ok(errors) = self.wire.take_errors() {
            for error in errors.into_iter().filter(|error| !is_link_lost(error)) {
                self.push(LinkEvent::Error(error));
            }
        }
        self.open.set(true);
        self.push(LinkEvent::Opened {
            info: self.info.clone(),
        });
    }

    /// Close and SAY so, even if nothing was open.
    async fn close_link(&self, reason: &str) {
        self.open.set(false);
        self.wire.disconnect().await;
        self.push(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Queue one request on the session's link (sent by its loop).
    fn send_json(&self, json: &str) {
        if !self.open.get() {
            return self.push(LinkEvent::Error(
                "write on a link that is not open".to_string(),
            ));
        }
        if let Err(error) = self.wire.send_client_json(json) {
            self.push(LinkEvent::Error(format!(
                "{} write failed: {error}",
                self.kind()
            )));
        }
    }

    /// Drain the session: errors first, then what the link read, then the
    /// link's own notes.
    fn pump(&self) {
        if !self.open.get() {
            return;
        }
        if let Ok(errors) = self.wire.take_errors() {
            for error in errors {
                let lost = is_link_lost(&error);
                self.push(LinkEvent::Error(error.clone()));
                if lost && self.open.replace(false) {
                    self.push(LinkEvent::Closed { reason: error });
                    return;
                }
            }
        }
        for read in self.wire.take_reads() {
            self.push(demux_read(read));
        }
        for note in self.wire.take_notes() {
            self.push(LinkEvent::WireNote(note));
        }
    }

    /// The first word of what this link says: `relay` or `wi-fi`.
    fn kind(&self) -> &'static str {
        if self.info.endpoint.is_relay() {
            "relay"
        } else {
            "wi-fi"
        }
    }

    fn push(&self, event: LinkEvent) {
        self.events.borrow_mut().push_back(event);
    }
}
