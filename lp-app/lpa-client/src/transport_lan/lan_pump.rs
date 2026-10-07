//! The LAN transport's I/O thread: one [`LanLink`] for the connection's
//! whole life, the loop the serial transport's `link_pump` runs, on frames
//! instead of bytes.
//!
//! 1. requests from the client go into the link, one proto message each
//!    (held here while the link's send budget is full);
//! 2. frames go out, frames come in (waiting no longer than the link's next
//!    timer), frames go out;
//! 3. what the board said goes to the client.
//!
//! A LAN session is never resumed: the board closes a keyed link whose
//! session resets (a new session is a new server link, with a new grant), so
//! a reset fails what was in flight ([`SerialInbound::LinkReset`]) and the
//! closed socket that follows ends the thread, which the transport reports
//! as the connection lost.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use lpc_wire::lp_link::SendError;
use lpc_wire::messages::ClientMessage;
use lpc_wire::{PortRead, ServerHello};
use tokio::sync::{mpsc, oneshot};

use super::lan_error::LanError;
use super::lan_link::{LanLink, LanOptions, LanSession};
use super::lan_target::LanTarget;
use crate::link_reset::reset_reason_words;
use crate::transport_serial::SerialInbound;

/// The longest the loop waits for frames before looking at new requests.
const MAX_WAIT: Duration = Duration::from_millis(10);

/// Everything the thread owns.
pub(super) struct LanPump {
    pub target: LanTarget,
    pub options: LanOptions,
    pub client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    pub server_tx: mpsc::UnboundedSender<SerialInbound>,
    pub shutdown_rx: oneshot::Receiver<()>,
    pub link_generation: Arc<AtomicU32>,
    /// Told how the open went, once.
    pub opened: oneshot::Sender<Result<ServerHello, LanError>>,
}

impl LanPump {
    /// Open the link, say how that went, then pump until shutdown or until
    /// the link is gone.
    pub(super) fn run(self) {
        let LanPump {
            target,
            options,
            client_rx,
            server_tx,
            shutdown_rx,
            link_generation,
            opened,
        } = self;
        let session = match LanLink::open(&target, &options) {
            Ok(session) => session,
            Err(error) => {
                let _ = opened.send(Err(error));
                return;
            }
        };
        let LanSession {
            link, hello, early, ..
        } = session;
        if opened.send(Ok(hello)).is_err() {
            link.close();
            return;
        }
        let mut pump = Pump {
            link,
            label: target.to_string(),
            client_rx,
            server_tx,
            shutdown_rx,
            link_generation,
            backlog: VecDeque::new(),
            outstanding: BTreeSet::new(),
        };
        for read in early {
            if !pump.deliver(read) {
                break;
            }
        }
        if let Err(error) = pump.run_loop() {
            log::info!("LAN link {}: {error}", pump.label);
        }
        pump.link.close();
        // Dropping `server_tx` (with `pump`) tells the transport the
        // connection is gone.
    }
}

struct Pump {
    link: LanLink,
    label: String,
    client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    server_tx: mpsc::UnboundedSender<SerialInbound>,
    shutdown_rx: oneshot::Receiver<()>,
    link_generation: Arc<AtomicU32>,
    /// Requests the link had no room for yet, in order.
    backlog: VecDeque<ClientMessage>,
    /// Ids sent and not yet answered: what a reset would lose.
    outstanding: BTreeSet<u64>,
}

impl Pump {
    /// `Ok` on shutdown (or nobody listening); `Err` when the link failed.
    fn run_loop(&mut self) -> Result<(), LanError> {
        loop {
            if self.shutdown_rx.try_recv().is_ok() {
                return Ok(());
            }
            while let Ok(message) = self.client_rx.try_recv() {
                self.backlog.push_back(message);
            }
            if !self.feed_requests() {
                return Ok(());
            }
            let wait = if self.backlog.is_empty() {
                MAX_WAIT
            } else {
                Duration::from_millis(1)
            };
            self.link.step(wait)?;
            while let Some(read) = self.link.port().poll_read() {
                if !self.deliver(read) {
                    return Ok(());
                }
            }
        }
    }

    /// Move held requests into the link while it has room. `false` when
    /// nobody is listening any more.
    fn feed_requests(&mut self) -> bool {
        while let Some(message) = self.backlog.front() {
            match self.link.port().send_client(message) {
                Ok(()) => {
                    self.outstanding.insert(message.id);
                    self.backlog.pop_front();
                }
                Err(SendError::Full) => break,
                Err(_) => {
                    let id = message.id;
                    self.backlog.pop_front();
                    let detail = format!("request id={id} is larger than {} carries", self.label);
                    log::warn!("LAN link: {detail}");
                    if self
                        .server_tx
                        .send(SerialInbound::SendFailed(detail))
                        .is_err()
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Hand one read on. `false` when nobody is listening.
    fn deliver(&mut self, read: PortRead) -> bool {
        match read {
            PortRead::Message(payload) => match payload.message {
                Ok(message) => {
                    self.outstanding.remove(&message.id);
                    self.server_tx.send(SerialInbound::Message(message)).is_ok()
                }
                Err(error) => {
                    log::warn!(
                        "LAN link {}: a message did not parse ({error}): {}",
                        self.label,
                        payload.json
                    );
                    true
                }
            },
            PortRead::Reset { reason } => {
                let generation = self.link.port().link().generation();
                self.link_generation.store(generation, Ordering::SeqCst);
                let words = reset_reason_words(reason);
                log::info!("LAN link {}: session reset: {words}", self.label);
                if self.outstanding.is_empty() {
                    return true;
                }
                self.outstanding.clear();
                self.server_tx
                    .send(SerialInbound::LinkReset {
                        generation,
                        detail: words.to_string(),
                    })
                    .is_ok()
            }
            // A LAN link carries no log records; a line here is news.
            PortRead::Log(line) => {
                log::debug!("LAN link {}: {line}", self.label);
                true
            }
            PortRead::Note(note) => {
                log::info!("LAN link {}: {note}", self.label);
                true
            }
            PortRead::Up { generation } => {
                log::debug!("LAN link {}: up (session {generation})", self.label);
                true
            }
        }
    }
}
