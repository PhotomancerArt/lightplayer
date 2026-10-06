//! [`Link`] over a Web Bluetooth (NUS) session (wasm only).
//!
//! The Bluetooth twin of `browser_serial`: the JS module owns the device and
//! the connection, this adapter turns that promise-shaped surface into the
//! model's event-queue contract. Connect and close are awaited in a spawned
//! future, drained one command at a time so a `Close` cannot overtake the
//! `Open` it follows (invariant I7: the fold never awaits device IO).
//!
//! # What differs from a serial port
//!
//! - **The link is usually already up when the model opens it.** A picked
//!   device is connected by the chooser flow, and a remembered one by the
//!   page-load restore, so `Open` is normally just "start listening"; only a
//!   link the model closed earlier pays for a (bounded) GATT connect.
//! - **`Close` really disconnects.** A held BLE connection is air time the
//!   board shares with ESP-NOW (G1 Run G), so a link Studio is not using
//!   does not stay connected. The session stays known, so opening again
//!   needs no chooser.
//! - **There is no reset.** No DTR/RTS on a GATT link: a `RunReset` fails by
//!   name and never pretends to have rebooted anything.
//! - **A drop is a departure.** The JS reports it as `bluetooth link lost: …`,
//!   which the effects layer's pump reads as the link being gone (like an
//!   unplug); the session's reconnect loop then announces the device again
//!   through the presence edge, and the sweep re-attaches it. A GATT
//!   disconnect is also the lp-link session's end, on both sides at once —
//!   Bluetooth's link reset — so the reconnect is a new session with a new
//!   hello.
//!
//! # The wire is lp-link, as over Web Serial
//!
//! Since `WIRE_PROTO_VERSION` 33 each connection is an lp-link on
//! `LinkConfig::ble()`'s datagrams, serviced by the provider
//! (`providers/browser_ble/ble_link_port.rs`, the same `LinkPortService`
//! Web Serial runs per port). A request is one link message
//! (`BleWire::send_client_json`); what the board said comes back as
//! [`WireRead`](crate::device_link::wire_reader::WireRead)s, demuxed exactly
//! as Web Serial's are; the link's own notes (up, a stall, the opt-in's
//! outcome) and a reset within a connection reach the journal as
//! `LinkEvent::WireNote`s (the USB cut-over's D9).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_devices::identity::{BLE_ENDPOINT_PREFIX, EndpointKey};
use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo};
use wasm_bindgen_futures::spawn_local;

use crate::device_link::demux::demux_read;
use crate::device_link::wire::client_message;
use crate::providers::browser_ble::{BleDevice, BleWire, is_link_lost};

/// The [`LinkInfo`] a Bluetooth device's link wears: its advertised name,
/// and the `ble:<device id>` endpoint. No USB ids and no serial number —
/// identity comes from the hello's base MAC, like every other transport.
pub fn ble_link_info(device: &BleDevice) -> LinkInfo {
    LinkInfo {
        label: device.name.clone(),
        endpoint: EndpointKey(format!("{BLE_ENDPOINT_PREFIX}{}", device.device_id)),
        usb: None,
        serial_number: None,
        carries_update_channel: false,
    }
}

/// One [`Link`] over a Bluetooth session.
pub struct BrowserBleLink {
    inner: Rc<BleLinkInner>,
}

impl BrowserBleLink {
    /// Wrap a session's wire. Nothing is read until the model opens it.
    pub fn new(wire: Rc<BleWire>, info: LinkInfo) -> Self {
        Self {
            inner: Rc::new(BleLinkInner {
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

impl Link for BrowserBleLink {
    fn info(&self) -> &LinkInfo {
        &self.inner.info
    }

    fn submit(&mut self, command: LinkCommand) {
        // Writes are queued in JS synchronously (it serializes and awaits
        // them itself), but they must not jump an `Open` whose connect is
        // still in flight — so every command waits in the same queue.
        self.inner.queue.borrow_mut().push_back(command);
        BleLinkInner::drain(&self.inner);
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.inner.events.borrow().is_empty() {
            self.inner.pump();
        }
        self.inner.events.borrow_mut().pop_front()
    }
}

struct BleLinkInner {
    wire: Rc<BleWire>,
    info: LinkInfo,
    events: RefCell<VecDeque<LinkEvent>>,
    queue: RefCell<VecDeque<LinkCommand>>,
    open: Cell<bool>,
    draining: Cell<bool>,
}

impl BleLinkInner {
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
                self.push(LinkEvent::Error(
                    "a Bluetooth link has no reset lines; reset needs USB".to_string(),
                ));
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
            // (`M!{json}`): on the link it is one message. Anything else is
            // not a request, and the link carries no raw text to the board.
            LinkCommand::SendLine(line) => match line.trim_end().strip_prefix("M!") {
                Some(json) => self.send_json(json),
                None => self.push(LinkEvent::Error(format!(
                    "not a request, and the link carries no raw text to the board: {line:?}"
                ))),
            },
            // Bluetooth runs lp-link now but opens no channel 3 yet (the
            // update plan's M7 P12 adds it), and its `LinkInfo` says so: the
            // model never asks. Dropped.
            LinkCommand::SendUpdate(_) => {}
        }
    }

    /// Start listening, connecting first if the session is not up. The baud
    /// the model asks for means nothing over GATT and is dropped.
    async fn open_link(&self) {
        if !self.wire.is_connected()
            && let Err(error) = self.wire.connect().await
        {
            self.push(LinkEvent::Error(format!(
                "bluetooth connect failed: {error}"
            )));
            return;
        }
        // What the board said since the connect waits, read, in the
        // session's link (its hello among them); an earlier connection's
        // reads went with that connection's link.
        self.open.set(true);
        self.push(LinkEvent::Opened {
            info: self.info.clone(),
        });
    }

    /// Disconnect and SAY so, even if nothing was open (the serial link's
    /// rule: a silent close makes the model wait out its cancel grace).
    async fn close_link(&self, reason: &str) {
        self.open.set(false);
        self.wire.disconnect().await;
        self.push(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Queue one request on the session's link (written by its loop).
    fn send_json(&self, json: &str) {
        if !self.open.get() {
            return self.push(LinkEvent::Error(
                "write on a link that is not open".to_string(),
            ));
        }
        if let Err(error) = self.wire.send_client_json(json) {
            self.push(LinkEvent::Error(format!("bluetooth write failed: {error}")));
        }
    }

    /// Drain the session: errors first (in the order they happened), then
    /// what the link read, then the link's own notes.
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
        // After the reads: a note (the board's answer to the opt-in, say) is
        // made while reading, and belongs after what it was read beside.
        for note in self.wire.take_notes() {
            self.push(LinkEvent::WireNote(note));
        }
    }

    fn push(&self, event: LinkEvent) {
        self.events.borrow_mut().push_back(event);
    }
}
