//! [`Link`] over the shipped browser Web Serial machinery (wasm only).
//!
//! This is a WRAPPER, not a second transport. The JS controller
//! (`browser_esp32_device_controller.js`) still owns the port, the read pump
//! and the line splitting; [`BrowserSerialEsp32Provider`] still owns the
//! endpoint, session and grant lifecycle. All this adapter does is turn that
//! promise-shaped surface into the model's event-queue contract.
//!
//! # The executor lives here, not in the model
//!
//! Web Serial is promise-shaped, so every command runs in a spawned future
//! that pushes [`LinkEvent`]s onto a shared queue (invariant I7: the model's
//! fold loop never awaits device IO — that is what kills the wedged-page
//! class). Commands are drained **one at a time** so a queued `Close` cannot
//! overtake the `Open` it follows.
//!
//! # Reading is free
//!
//! Lines and errors come back through the provider's synchronous
//! `take_lines`/`take_errors`, so [`Link::poll_event`] needs no future at
//! all: it drains what the JS read pump has already buffered and demuxes it.
//! A JS controller error means the port died underneath us (the shipped
//! `mark_gone` rule), so it surfaces as an error AND closes the link.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, ResetKind};
use wasm_bindgen_futures::spawn_local;

use crate::device_link::demux::demux_line;
use crate::device_link::wire::client_message;
use crate::provider::endpoint::LinkEndpointId;
use crate::provider::session::LinkSessionId;
use crate::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
use crate::{LinkError, LinkProvider};

/// One [`Link`] over a granted Web Serial port.
///
/// Cheap to clone-adjacent: the provider is shared (it owns the port), so
/// building a link never takes a port away from the rest of the app.
pub struct BrowserSerialLink {
    inner: Rc<BrowserLinkInner>,
}

impl BrowserSerialLink {
    /// Wrap a granted endpoint on a shared provider.
    ///
    /// Nothing touches the port until the model sends `LinkCommand::Open`:
    /// holding a grant is not the same as being connected, and the model is
    /// the only thing entitled to decide the difference.
    pub fn new(
        provider: Rc<BrowserSerialEsp32Provider>,
        endpoint: LinkEndpointId,
        info: LinkInfo,
    ) -> Self {
        Self {
            inner: Rc::new(BrowserLinkInner {
                provider,
                endpoint,
                info,
                events: RefCell::new(VecDeque::new()),
                queue: RefCell::new(VecDeque::new()),
                session: RefCell::new(None),
                open: Cell::new(false),
                baud: Cell::new(0),
                draining: Cell::new(false),
            }),
        }
    }

    /// Whether the protocol port is open for traffic right now.
    pub fn is_open(&self) -> bool {
        self.inner.open.get()
    }
}

impl Link for BrowserSerialLink {
    fn info(&self) -> &LinkInfo {
        &self.inner.info
    }

    fn submit(&mut self, command: LinkCommand) {
        self.inner.queue.borrow_mut().push_back(command);
        BrowserLinkInner::drain(&self.inner);
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.inner.events.borrow().is_empty() {
            self.inner.pump_lines();
        }
        self.inner.events.borrow_mut().pop_front()
    }
}

/// Shared state: what the spawned futures and the polling side both touch.
struct BrowserLinkInner {
    provider: Rc<BrowserSerialEsp32Provider>,
    endpoint: LinkEndpointId,
    info: LinkInfo,
    events: RefCell<VecDeque<LinkEvent>>,
    queue: RefCell<VecDeque<LinkCommand>>,
    session: RefCell<Option<LinkSessionId>>,
    open: Cell<bool>,
    /// The baud the model last asked for, so a reset re-opens at the same
    /// rate. The app link runs at 921600 and bootloader logs at 115200, and
    /// re-opening at the wrong one turns a clean `[INIT]` banner into
    /// binary splat.
    baud: Cell<u32>,
    /// A future is already draining [`Self::queue`]. Keeps commands ordered
    /// without a channel.
    draining: Cell<bool>,
}

impl BrowserLinkInner {
    /// Start draining the command queue, unless a future already is.
    fn drain(inner: &Rc<Self>) {
        if inner.draining.get() {
            return;
        }
        inner.draining.set(true);
        let inner = Rc::clone(inner);
        spawn_local(async move {
            loop {
                // The borrow ends before the await: no RefCell borrow may
                // span a suspension point (the provider's own rule).
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
            LinkCommand::Open { baud } => self.open_port(baud).await,
            LinkCommand::Close => self.close_port("closed by request").await,
            LinkCommand::RunReset(kind) => self.run_reset(kind).await,
            LinkCommand::SendFrame(frame) => match client_message(&frame) {
                Ok(message) => match lpc_wire::json::to_string(&message) {
                    Ok(json) => self.write_line(&format!("M!{json}\n")).await,
                    Err(error) => self.push(LinkEvent::Error(format!(
                        "failed to encode {:?}: {error}",
                        frame.body
                    ))),
                },
                Err(error) => self.push(LinkEvent::Error(error)),
            },
            LinkCommand::SendLine(line) => self.write_line(&format!("{line}\n")).await,
        }
    }

    /// Connect a session (if there is not one) and open the protocol port.
    ///
    /// The JS `openProtocol` performs a normal hard reset as part of opening,
    /// which is why the fold treats `Opened` as the start of a fresh
    /// observation window: whatever we concluded about the previous
    /// generation describes a machine that no longer exists.
    async fn open_port(&self, baud: u32) {
        let session = match self.session.borrow().clone() {
            Some(session) => session,
            None => match self.provider.connect(&self.endpoint).await {
                Ok(session) => {
                    *self.session.borrow_mut() = Some(session.id.clone());
                    session.id
                }
                Err(error) => return self.fail("connect", &error),
            },
        };
        self.baud.set(baud);
        match self.provider.open_protocol(&session, baud).await {
            Ok(()) => {
                self.open.set(true);
                self.push(LinkEvent::Opened {
                    info: self.info.clone(),
                });
            }
            Err(error) => self.fail("open", &error),
        }
    }

    /// Close the port and SAY so, even if there was nothing open.
    ///
    /// Every `Close` gets an answer on purpose: cancelling identification is
    /// "give the port back, tell me when it is back", and a link that stays
    /// silent because there was nothing to close makes the model wait out its
    /// whole cancel grace before evicting.
    async fn close_port(&self, reason: &str) {
        self.open.set(false);
        // Release the reader/writer AND the provider session. The GRANT
        // survives: revoking it is `forget_endpoint`'s job, which the model
        // asks for separately through `Command::RevokeGrant`.
        if let Some(session) = self.session.borrow().clone() {
            if let Err(error) = self.provider.release_protocol(&session).await {
                self.fail("release", &error);
            }
            if let Err(error) = self.provider.close(&session).await {
                self.fail("close", &error);
            }
            *self.session.borrow_mut() = None;
        }
        self.push(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Run a reset through the shipped controller.
    ///
    /// Only [`ResetKind::Normal`] is reachable from here today, and it is
    /// reached the way the shipped code does it: re-opening the protocol
    /// port, whose `openProtocol` runs `D0 W100 R1 W100 R0` — the normal
    /// ESP32 hard reset — and leaves the boot output in the line buffer where
    /// [`Link::poll_event`] picks it up as diagnosis.
    ///
    /// The other kinds exist in the JS controller's `runReset` but are not
    /// plumbed through `browser_serial.js`'s `resetAndRead`, and
    /// [`ResetKind::BothThenDrop`] (the CH34x sequence) is not in the
    /// controller at all. Refusing loudly beats silently performing a
    /// DIFFERENT reset than the one asked for: a caller that thinks it put a
    /// board into the ROM downloader and did not would misread everything
    /// after.
    async fn run_reset(&self, kind: ResetKind) {
        if kind != ResetKind::Normal {
            self.push(LinkEvent::Error(format!(
                "{kind:?} is not plumbed through browser Web Serial yet; only Normal is"
            )));
            self.push(LinkEvent::ResetOutcome { kind, ok: false });
            return;
        }
        let Some(session) = self.session.borrow().clone() else {
            self.push(LinkEvent::Error(
                "reset on a link with no session".to_string(),
            ));
            self.push(LinkEvent::ResetOutcome { kind, ok: false });
            return;
        };
        let baud = self.baud.get();
        if let Err(error) = self.provider.release_protocol(&session).await {
            self.fail("release", &error);
        }
        self.open.set(false);
        match self.provider.open_protocol(&session, baud).await {
            Ok(()) => {
                self.open.set(true);
                self.push(LinkEvent::ResetOutcome { kind, ok: true });
                self.push(LinkEvent::Opened {
                    info: self.info.clone(),
                });
            }
            Err(error) => {
                self.fail("reset", &error);
                self.push(LinkEvent::ResetOutcome { kind, ok: false });
            }
        }
    }

    async fn write_line(&self, line: &str) {
        let Some(session) = self.session.borrow().clone() else {
            return self.push(LinkEvent::Error(
                "write on a link that is not open".to_string(),
            ));
        };
        if let Err(error) = self.provider.write_line(&session, line).await {
            self.fail("write", &error);
        }
    }

    /// Drain the JS read pump's buffers onto the event queue.
    fn pump_lines(&self) {
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        if let Ok(lines) = self.provider.take_lines(&session) {
            for line in lines {
                self.push(demux_line(&line));
            }
        }
        let Ok(errors) = self.provider.take_errors(&session) else {
            return;
        };
        if errors.is_empty() {
            return;
        }
        for error in errors {
            self.push(LinkEvent::Error(format!("browser serial error: {error}")));
        }
        // The JS controller only reports errors when the port failed under
        // it (a cancelled read, a vanished device). Treating that as merely
        // "an error" is how a dead port used to keep looking alive.
        if self.open.replace(false) {
            self.push(LinkEvent::Closed {
                reason: "browser serial error".to_string(),
            });
        }
    }

    fn fail(&self, operation: &str, error: &LinkError) {
        self.push(LinkEvent::Error(format!("{operation} failed: {error}")));
    }

    fn push(&self, event: LinkEvent) {
        self.events.borrow_mut().push_back(event);
    }
}
