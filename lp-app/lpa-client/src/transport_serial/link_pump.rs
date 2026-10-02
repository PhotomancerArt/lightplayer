//! The serial transport's I/O thread: one lp-link over one byte stream.
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB serial link is an lp-link
//! (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`). This thread owns the
//! stream and one [`WireLinkPort`] for the stream's whole life, and runs the
//! loop a link edge runs:
//!
//! 1. requests from the client go into the link, one proto message each
//!    (held here while the link's send budget is full);
//! 2. the frames the link wants written are written;
//! 3. bytes from the stream go into the link, waiting no longer than its
//!    next timer;
//! 4. what the board said comes out: wire messages to the client, console
//!    lines (log records and raw text) to the line observer and stderr.
//!
//! A frame the stream would not take ([`ByteStreamError::WriteStalled`]) is
//! simply dropped: the link resends it. A link reset with requests in flight
//! fails them at once ([`SerialInbound::LinkReset`], D9).

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use lpc_wire::lp_link::{LinkConfig, SendError};
use lpc_wire::messages::ClientMessage;
use lpc_wire::{PortRead, WireEncoding, WireLinkPort};
use tokio::sync::{mpsc, oneshot};

use super::client::SerialInbound;
use super::hardware::HardwareSerialOptions;
use super::link_nonce::fresh_link_nonce;
use crate::link_reset::reset_reason_words;
use crate::stream::{ByteStreamError, DeviceByteStream};

/// The longest the loop waits for bytes before looking at new requests
/// again: the latency a request can pick up between `send` and the wire.
const MAX_WAIT: Duration = Duration::from_millis(10);

/// Bytes read per pass.
const READ_CHUNK: usize = 4096;

/// Everything the thread owns.
pub(super) struct LinkPump {
    pub stream: Box<dyn DeviceByteStream>,
    pub stream_label: String,
    pub client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    pub server_tx: mpsc::UnboundedSender<SerialInbound>,
    pub shutdown_rx: oneshot::Receiver<()>,
    pub link_generation: Arc<AtomicU32>,
    pub options: HardwareSerialOptions,
}

/// How the loop ended.
enum Ending {
    /// Closed by the transport (or nobody is listening any more).
    Shutdown,
    /// The stream failed: the connection is gone.
    Lost,
}

impl LinkPump {
    /// Run until shutdown or until the stream fails, on a link built from
    /// `config`. Returns the stream so the caller decides when the OS
    /// resource is released.
    pub(super) fn run(mut self, config: LinkConfig) -> Box<dyn DeviceByteStream> {
        let want_packed = self
            .options
            .wire_encoding
            .unwrap_or_else(crate::wire_encoding_env::requested_wire_encoding)
            == WireEncoding::Packed;
        let mut state = PumpState {
            port: WireLinkPort::new(config, fresh_link_nonce(), want_packed),
            started: Instant::now(),
            backlog: VecDeque::new(),
            outstanding: BTreeSet::new(),
            stalled_writes: 0,
        };
        let ending = self.run_loop(&mut state);
        if let Ending::Lost = ending {
            log::debug!("Serial thread: {} lost", self.stream_label);
        }
        // Dropping `server_tx` (with `self`) is what tells the transport the
        // connection is gone.
        self.stream
    }

    fn run_loop(&mut self, state: &mut PumpState) -> Ending {
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            if self.shutdown_rx.try_recv().is_ok() {
                log::debug!("Serial thread: Shutdown signal received");
                return Ending::Shutdown;
            }
            // 1. Requests into the link.
            while let Ok(message) = self.client_rx.try_recv() {
                state.backlog.push_back(message);
            }
            if !self.feed_requests(state) {
                return Ending::Shutdown;
            }
            // 2. Frames out.
            if let Err(error) = self.write_frames(state) {
                log::error!(
                    "Serial thread: Write error on {}: {error}",
                    self.stream_label
                );
                return Ending::Lost;
            }
            // 3. Bytes in, waiting no longer than the link's next timer.
            let wait = state.wait();
            let asked = Instant::now();
            match self.stream.read_available_within(&mut buf, wait) {
                Ok(0) => {
                    // A stream whose reads never wait: sleep out the rest.
                    let spent = asked.elapsed();
                    if spent < wait && self.client_rx.is_empty() {
                        std::thread::sleep(wait - spent);
                    }
                }
                Ok(read) => {
                    let now = state.now();
                    state.port.on_bytes(now, &buf[..read]);
                }
                Err(error) => {
                    log::error!(
                        "Serial thread: Read error on {}: {error}",
                        self.stream_label
                    );
                    return Ending::Lost;
                }
            }
            // 4. What the board said.
            if !self.deliver_reads(state) {
                return Ending::Shutdown;
            }
        }
    }

    /// Move held requests into the link while it has room. `false` when
    /// nobody is listening any more.
    fn feed_requests(&mut self, state: &mut PumpState) -> bool {
        while let Some(message) = state.backlog.front() {
            match state.port.send_client(message) {
                Ok(()) => {
                    log::debug!(
                        "Serial thread: request id={} queued on {}",
                        message.id,
                        self.stream_label
                    );
                    state.outstanding.insert(message.id);
                    state.backlog.pop_front();
                }
                // The send budget is full: hold the rest until the board
                // acknowledges some.
                Err(SendError::Full) => break,
                Err(error) => {
                    let id = message.id;
                    state.backlog.pop_front();
                    let detail = format!(
                        "request id={id} could not be sent on {}: {error:?}",
                        self.stream_label
                    );
                    log::warn!("Serial thread: {detail}");
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

    /// Write every frame the link has for the stream. A frame the stream
    /// would not take is dropped (the link resends it) and ends the pass, so
    /// a board that stopped reading costs one write timeout per pass, not
    /// one per frame.
    fn write_frames(&mut self, state: &mut PumpState) -> Result<(), ByteStreamError> {
        let now = state.now();
        while let Some(frame) = state.port.poll_transmit(now) {
            match self.stream.write_all(frame) {
                Ok(()) => state.stalled_writes = 0,
                Err(ByteStreamError::WriteStalled) => {
                    state.stalled_writes += 1;
                    if state.stalled_writes == 1 {
                        log::warn!(
                            "Serial thread: {} is not accepting output; the link will resend",
                            self.stream_label
                        );
                    }
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Hand on everything the port read. `false` when nobody is listening.
    fn deliver_reads(&mut self, state: &mut PumpState) -> bool {
        while let Some(read) = state.port.poll_read() {
            match read {
                PortRead::Message(payload) => {
                    if let Some(observer) = &self.options.line_observer {
                        observer.observe_line(&format!("M!{}", payload.json));
                    }
                    match payload.message {
                        Ok(message) => {
                            log::debug!(
                                "Serial thread: server message id={} ({} B, {})",
                                message.id,
                                payload.wire_len,
                                if payload.packed { "packed" } else { "json" }
                            );
                            state.outstanding.remove(&message.id);
                            if self
                                .server_tx
                                .send(SerialInbound::Message(message))
                                .is_err()
                            {
                                log::debug!("Serial thread: server_tx closed, exiting");
                                return false;
                            }
                        }
                        Err(error) => log::warn!(
                            "Serial thread: a message from {} did not parse: {error} | json: {}",
                            self.stream_label,
                            payload.json
                        ),
                    }
                }
                PortRead::Log(line) => {
                    if let Some(observer) = &self.options.line_observer {
                        observer.observe_line(&line);
                    }
                    eprintln!("[serial] {line}");
                }
                PortRead::Up { generation } => {
                    log::debug!(
                        "Serial thread: link up on {} (session {generation})",
                        self.stream_label
                    );
                }
                PortRead::Reset { reason } => {
                    let generation = state.port.link().generation();
                    self.link_generation.store(generation, Ordering::SeqCst);
                    let words = reset_reason_words(reason);
                    log::info!(
                        "Serial thread: link on {} reset: {words}",
                        self.stream_label
                    );
                    if !state.outstanding.is_empty() {
                        state.outstanding.clear();
                        let lost = SerialInbound::LinkReset {
                            generation,
                            detail: words.to_string(),
                        };
                        if self.server_tx.send(lost).is_err() {
                            return false;
                        }
                    }
                }
                PortRead::Note(note) => {
                    log::info!("Serial thread: {}: {note}", self.stream_label);
                }
            }
        }
        true
    }
}

/// The loop's own state beside the stream.
struct PumpState {
    port: WireLinkPort,
    started: Instant,
    /// Requests the link had no room for yet, in order.
    backlog: VecDeque<ClientMessage>,
    /// Ids sent on the current session and not yet answered: what a reset
    /// would lose.
    outstanding: BTreeSet<u64>,
    /// Writes in a row the stream would not take.
    stalled_writes: u32,
}

impl PumpState {
    /// Microseconds since the port opened: the link's clock.
    fn now(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    /// How long a read may wait: until the link's next timer, at most
    /// [`MAX_WAIT`], and not at all while requests wait for room.
    fn wait(&self) -> Duration {
        if !self.backlog.is_empty() {
            return Duration::from_millis(1);
        }
        let now = self.now();
        match self.port.poll_timeout() {
            Some(at) => Duration::from_micros(at.saturating_sub(now)).min(MAX_WAIT),
            None => MAX_WAIT,
        }
    }
}
