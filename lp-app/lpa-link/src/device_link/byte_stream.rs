//! [`Link`] over the [`DeviceByteStream`] seam: commands in, events out, no
//! executor.
//!
//! This is the host-side half of M3's dependency inversion. The seam it wraps
//! is the same one the real serial transport drives
//! (`lpa_client::transport_serial::hardware`), so the fake device, a native
//! port, an emulated board over a socket and anything else that can be a byte pipe all
//! reach the model through one adapter.
//!
//! # The link underneath
//!
//! Since `WIRE_PROTO_VERSION` 30 a board's USB link is an lp-link (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`). Each open port gets one
//! [`LinkPortService`] (a `WireLinkPort`, a fresh nonce per open) for as
//! long as it is open — the same per-port service the browser's ports use:
//! the model's requests go in as proto messages, the frames the link wants
//! are written, and what the board said comes out through
//! `port_read_map` and [`demux_read`]: wire messages as frames (or an app
//! conversation's passthrough), console text as lines, and the link's own
//! story (up, stalled, the opt-in's outcome, a reset — which fails what was
//! in flight, D9) as wire notes. Lost or damaged bytes never reach the
//! model: the link resends them.
//!
//! # One link, two drainers
//!
//! While an effect or the editor lens borrows the wire (pause-the-pump, plan
//! D2), the model's pump stops calling [`Link::poll_event`] and the
//! borrower's conversation drains the SAME link through a
//! [`ByteStreamPort`] ([`ByteStreamLink::port_handle`]): requests in, the
//! board's reads out, undecoded by the model. There is never a second reader
//! of the bytes — two would each tear the frame that straddles the handover
//! — and the borrower leaves the transport's own events (`Opened`,
//! `Closed`, reset outcomes) queued for the model.
//!
//! # Why this can be synchronous
//!
//! [`DeviceByteStream`] is deliberately sync: `read_available` returns `Ok(0)`
//! rather than waiting. That is exactly the shape [`Link::poll_event`] wants
//! — "take the next event, if one is ready, never block" — so this adapter
//! needs no runtime at all; the link's timers run whenever someone polls
//! (the effects layer's pump does, every few tens of milliseconds). The
//! browser adapter cannot do this (Web Serial is promise-shaped) and spawns
//! futures instead; see `browser_serial`.
//!
//! # What it deliberately does not do
//!
//! - **No reset on open.** The real serial transport resets after opening a
//!   port, and the browser's `openProtocol` does too. Here the model asks:
//!   `LinkCommand::RunReset` exists precisely so a reboot is a decision with
//!   a journal line, not a side effect of connecting.
//! - **No classification.** Boot lines go out as [`LinkEvent::Line`] and
//!   frames as [`LinkEvent::Frame`]; the hello gate and the boot-line
//!   diagnosis live in the device fold, which is what keeps verdicts
//!   non-sticky.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, ResetKind};
use lpc_wire::{ClientMessage, LinkCounters};

use crate::device_link::demux::demux_read;
use crate::device_link::link_nonce::fresh_link_nonce;
use crate::device_link::link_port_service::LinkPortService;
use crate::device_link::wire::client_message;
use crate::device_link::wire_reader::WireRead;
use crate::stream::{ByteStreamError, DeviceByteStream};

/// Bytes read per `read_available` call.
const READ_CHUNK: usize = 4096;

/// Reads per pump. A firehose must not hold the caller: whatever is left is
/// read on the next [`Link::poll_event`], and the model is event-driven
/// anyway.
const READS_PER_PUMP: usize = 64;

/// A monotonic microsecond clock for the link's timers.
type LinkClock = Box<dyn FnMut() -> u64 + Send>;

/// One open (or opening) link over a byte stream.
pub struct ByteStreamLink<S: DeviceByteStream> {
    info: LinkInfo,
    core: Arc<Mutex<LinkCore<S>>>,
}

/// The same link, for a conversation that borrows the wire (see the module
/// docs). Cheap to clone; every clone is the one link.
pub struct ByteStreamPort<S: DeviceByteStream> {
    core: Arc<Mutex<LinkCore<S>>>,
}

impl<S: DeviceByteStream> Clone for ByteStreamPort<S> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

/// A [`ByteStreamPort`] that does not keep its link alive: for a registry
/// that must not outlive the link (the fake board remembers the last link
/// opened on it this way).
pub struct WeakByteStreamPort<S: DeviceByteStream> {
    core: Weak<Mutex<LinkCore<S>>>,
}

impl<S: DeviceByteStream> WeakByteStreamPort<S> {
    /// The port, while its link is alive.
    pub fn upgrade(&self) -> Option<ByteStreamPort<S>> {
        self.core.upgrade().map(|core| ByteStreamPort { core })
    }
}

/// Something waiting for the model, in order.
enum Queued {
    /// The transport's own event (opened, closed, an IO error, a reset's
    /// outcome).
    Event(LinkEvent),
    /// Something the board said, mapped when the model takes it.
    Read(WireRead),
}

/// Everything behind the link, shared by the model's pump and a borrower.
struct LinkCore<S: DeviceByteStream> {
    stream: S,
    /// The open port's link; `None` while closed.
    service: Option<LinkPortService>,
    /// Whether the port asks the board to pack its replies.
    want_packed: bool,
    clock: LinkClock,
    queue: VecDeque<Queued>,
    /// Why the port last closed, for a borrower that asks after.
    closed_because: Option<String>,
}

impl<S: DeviceByteStream> ByteStreamLink<S> {
    /// A link that is attached but not yet open. Nothing happens on the wire
    /// until the model sends `LinkCommand::Open`.
    pub fn new(info: LinkInfo, stream: S) -> Self {
        Self {
            info,
            core: Arc::new(Mutex::new(LinkCore {
                stream,
                service: None,
                want_packed: false,
                clock: default_clock(),
                queue: VecDeque::new(),
                closed_because: None,
            })),
        }
    }

    /// Ask the board to pack its replies (plan `lp-json-pack`) once its hello
    /// says it can with this build's pack format, once per link session;
    /// `now_ms` is any monotonic millisecond clock, and becomes the link's.
    /// The link reads both forms either way; this is only whether it asks.
    pub fn asking_for_packed_replies(
        self,
        mut now_ms: impl FnMut() -> u64 + Send + 'static,
    ) -> Self {
        {
            let mut core = self.core();
            core.want_packed = true;
            core.clock = Box::new(move || now_ms().saturating_mul(1_000));
        }
        self
    }

    /// Run the link's timers on `now_us` (any monotonic microsecond clock)
    /// instead of the default one. Tests use it to drive time by hand.
    pub fn with_clock_us(self, now_us: impl FnMut() -> u64 + Send + 'static) -> Self {
        self.core().clock = Box::new(now_us);
        self
    }

    /// Whether the port is currently open for traffic.
    pub fn is_open(&self) -> bool {
        self.core().service.is_some()
    }

    /// The open port's link counters (resends, damaged frames, resets…).
    pub fn link_counters(&self) -> Option<LinkCounters> {
        self.core().service.as_ref().map(LinkPortService::counters)
    }

    /// The same link, for a conversation that borrows the wire.
    pub fn port_handle(&self) -> ByteStreamPort<S> {
        ByteStreamPort {
            core: Arc::clone(&self.core),
        }
    }

    fn core(&self) -> MutexGuard<'_, LinkCore<S>> {
        lock(&self.core)
    }
}

impl<S: DeviceByteStream> ByteStreamPort<S> {
    /// A handle that does not keep the link alive.
    pub fn downgrade(&self) -> WeakByteStreamPort<S> {
        WeakByteStreamPort {
            core: Arc::downgrade(&self.core),
        }
    }

    /// Queue one request on the link and write what it produced.
    pub fn send(&self, message: &ClientMessage) -> Result<(), String> {
        let json = lpc_wire::json::to_string(message).map_err(|error| error.to_string())?;
        self.send_json(&json)
    }

    /// Queue one request, already JSON (no `M!`, no newline).
    pub fn send_json(&self, json: &str) -> Result<(), String> {
        lock(&self.core).send_json(json)
    }

    /// Service the link and take everything the board has said since the
    /// last take: wire messages, console lines, link resets. The link's
    /// notes and the transport's own events stay queued for the model. `Err`
    /// once the port has closed (after the reads that came before it).
    pub fn take_reads(&self) -> Result<Vec<WireRead>, String> {
        let mut core = lock(&self.core);
        core.pump();
        let mut reads = Vec::new();
        for queued in std::mem::take(&mut core.queue) {
            match queued {
                Queued::Read(read) => reads.push(read),
                Queued::Event(event) => core.queue.push_back(Queued::Event(event)),
            }
        }
        if reads.is_empty() && core.service.is_none() {
            return Err(core
                .closed_because
                .clone()
                .unwrap_or_else(|| "the link is not open".to_string()));
        }
        Ok(reads)
    }
}

impl<S: DeviceByteStream> LinkCore<S> {
    fn open_port(&mut self, info: &LinkInfo, baud: u32) {
        match self.stream.reopen(baud) {
            Ok(()) => {
                // A fresh port is a fresh link session (a new nonce), so the
                // board starts its per-link state over too.
                self.service = Some(LinkPortService::new(
                    lpc_wire::lp_link::LinkConfig::usb(),
                    fresh_link_nonce(),
                    self.want_packed,
                    None,
                ));
                self.closed_because = None;
                self.push_event(LinkEvent::Opened { info: info.clone() });
            }
            Err(error) => self.fail(&error),
        }
    }

    /// Close the port and SAY so, even if it was not open.
    ///
    /// Every `Close` gets an answer on purpose: cancelling identification is
    /// "give the port back, tell me when it is back", and a link that stays
    /// silent because there was nothing to close makes the model wait out its
    /// whole cancel grace before evicting. Bounded, but two seconds of
    /// pointless "cancelling…".
    fn close_port(&mut self, reason: &str) {
        self.service = None;
        self.closed_because = Some(reason.to_string());
        self.push_event(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Queue one request (bare JSON) on the link and write what it produced.
    fn send_json(&mut self, json: &str) -> Result<(), String> {
        let Some(service) = self.service.as_mut() else {
            return Err("write on a link that is not open".to_string());
        };
        service.send_client_json(json)?;
        self.write_frames();
        Ok(())
    }

    /// Write every frame the link has for the stream.
    ///
    /// A port that answers a peer that has stopped reading with
    /// `ByteStreamError::WriteStalled` dropped a frame, it did not fail: the
    /// link resends it, so the pass just ends (defect
    /// `docs/defects/2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md`).
    fn write_frames(&mut self) {
        let now = (self.clock)();
        let Self {
            stream, service, ..
        } = self;
        let Some(service) = service.as_mut() else {
            return;
        };
        // After the first frame the stream would not take, the rest of this
        // pass is dropped too (the link resends them all).
        let mut failed: Option<ByteStreamError> = None;
        service.transmit(now, |frame| {
            if failed.is_none()
                && let Err(error) = stream.write_all(frame)
            {
                failed = Some(error);
            }
        });
        match failed {
            None | Some(ByteStreamError::WriteStalled) => {}
            Some(ByteStreamError::Closed) => {
                self.drain_service();
                self.close_port("device disconnected");
            }
            Some(error) => self.fail(&error),
        }
    }

    /// Run one reset dance as single-pin writes.
    ///
    /// The sequences mirror the shipped ones — the browser controller's
    /// `runReset` kinds and `lpa_client::transport_serial::hardware`'s
    /// `reset_after_open` — but with **no inter-step delays**: this adapter
    /// may not block the caller, and its M3 instantiation is the fake device,
    /// which keys on the pin EDGES (RTS falling, DTR ever high) and not on
    /// their timing. Real silicon needs the ~100 ms holds, so a host-serial
    /// instantiation must drive its reset from a thread that can sleep rather
    /// than from here.
    ///
    /// The link needs nothing from a reset: a rebooted board comes back with
    /// a new nonce, and the link resets itself (a `link reset` error, then
    /// the board's hello on the new session).
    fn run_reset(&mut self, kind: ResetKind) {
        let steps: &[(Option<bool>, Option<bool>)] = match kind {
            // "D0 W100 R1 W100 R0": IO0 high, hold EN low, release.
            ResetKind::Normal => &[(Some(false), None), (None, Some(true)), (None, Some(false))],
            ResetKind::RtsOnly => &[(None, Some(true)), (None, Some(false))],
            // "R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0": the native
            // USB-Serial-JTAG pattern that selects the ROM downloader.
            ResetKind::UsbJtagDownload => &[
                (None, Some(false)),
                (Some(false), None),
                (Some(true), None),
                (None, Some(false)),
                (None, Some(true)),
                (Some(false), None),
                (None, Some(true)),
                (None, Some(false)),
                (Some(false), None),
            ],
            // The CH34x lore: whole-status writes only, because the WCH
            // macOS driver ignores single-bit calls, and never crossing
            // (DTR asserted, RTS released) — that pattern selects the ROM
            // bootloader instead of rebooting the app.
            ResetKind::BothThenDrop => &[
                (Some(false), Some(false)),
                (Some(true), Some(true)),
                (Some(false), Some(true)),
                (Some(false), Some(false)),
            ],
        };
        for (dtr, rts) in steps {
            if let Err(error) = self.stream.set_signals(*dtr, *rts) {
                self.fail(&error);
                self.push_event(LinkEvent::ResetOutcome { kind, ok: false });
                return;
            }
        }
        self.push_event(LinkEvent::ResetOutcome { kind, ok: true });
    }

    /// Service the link: write what it has, read whatever the wire has, and
    /// queue what the board said.
    fn pump(&mut self) {
        if self.service.is_none() {
            return;
        }
        self.write_frames();
        let mut buf = [0u8; READ_CHUNK];
        for _ in 0..READS_PER_PUMP {
            let now = (self.clock)();
            let Some(service) = self.service.as_mut() else {
                return;
            };
            match self.stream.read_available(&mut buf) {
                Ok(0) => break,
                Ok(read) => service.on_bytes(now, &buf[..read]),
                Err(ByteStreamError::Closed) => {
                    self.drain_service();
                    self.close_port("device disconnected");
                    return;
                }
                Err(error) => {
                    self.fail(&error);
                    return;
                }
            }
        }
        // Acknowledgements (and the opt-in) for what was just read.
        self.write_frames();
        self.drain_service();
    }

    /// Queue what the link has read (for whoever drains) and its notes (for
    /// the model).
    fn drain_service(&mut self) {
        let Some(service) = self.service.as_mut() else {
            return;
        };
        // Reads first: a note (the opt-in's outcome, say) is made while
        // reading, and belongs after what it was read beside.
        let reads = service.take_reads();
        let notes = service.take_notes();
        self.queue.extend(reads.into_iter().map(Queued::Read));
        self.queue.extend(
            notes
                .into_iter()
                .map(|note| Queued::Event(LinkEvent::WireNote(note))),
        );
    }

    /// The model's next event, if one is queued.
    fn next_event(&mut self) -> Option<LinkEvent> {
        while let Some(queued) = self.queue.pop_front() {
            match queued {
                Queued::Event(event) => return Some(event),
                Queued::Read(read) => return Some(demux_read(read)),
            }
        }
        None
    }

    fn push_event(&mut self, event: LinkEvent) {
        self.queue.push_back(Queued::Event(event));
    }

    /// Surface an IO failure as an event. Never a return value: the model is
    /// not where IO errors are decided.
    fn fail(&mut self, error: &ByteStreamError) {
        self.push_event(LinkEvent::Error(error.to_string()));
    }
}

impl<S: DeviceByteStream> Link for ByteStreamLink<S> {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        let info = self.info.clone();
        let mut core = self.core();
        match command {
            LinkCommand::Open { baud } => core.open_port(&info, baud),
            LinkCommand::Close => core.close_port("closed by request"),
            LinkCommand::RunReset(kind) => core.run_reset(kind),
            LinkCommand::SendFrame(frame) => {
                let sent = client_message(&frame)
                    .and_then(|message| {
                        lpc_wire::json::to_string(&message).map_err(|error| error.to_string())
                    })
                    .and_then(|json| core.send_json(&json));
                if let Err(error) = sent {
                    core.push_event(LinkEvent::Error(error));
                }
            }
            // A conversation's request, as the `M!{json}` line it always
            // wrote: the link carries the JSON.
            LinkCommand::SendLine(line) => {
                let line = line.trim_end_matches(['\r', '\n']);
                let sent = match line.strip_prefix("M!") {
                    Some(json) => core.send_json(json),
                    None => Err(format!(
                        "only `M!` request lines can be sent on a device link: {line:?}"
                    )),
                };
                if let Err(error) = sent {
                    core.push_event(LinkEvent::Error(error));
                }
            }
            // This transport has no channel 3 yet (M7 P7 adds it), and its
            // `LinkInfo` says so, so the model never asks; dropped.
            LinkCommand::SendUpdate(_) => {}
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        let mut core = self.core();
        if let Some(event) = core.next_event() {
            return Some(event);
        }
        core.pump();
        core.next_event()
    }
}

fn lock<S: DeviceByteStream>(core: &Mutex<LinkCore<S>>) -> MutexGuard<'_, LinkCore<S>> {
    core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The link's clock when the caller gives none: microseconds since the link
/// was made.
fn default_clock() -> LinkClock {
    let started = std::time::Instant::now();
    Box::new(move || u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;
    use lpa_devices::identity::EndpointKey;
    use lpa_devices::wire::{ClientFrame, ServerFrameBody};
    use lpc_wire::lp_link::{CH_PROTO, LinkConfig, SelectiveRepeat};
    use lpc_wire::{ClientMessage, ServerMsgBody, WireServerMessage};

    /// A closed link is silent: nothing is read before the model opens the
    /// port, so a grant that is merely held produces no evidence.
    #[test]
    fn nothing_flows_until_the_model_opens_the_port() {
        let (mut link, board) = link_and_board();
        board.say("[INIT] booting\n");

        assert_eq!(link.poll_event(), None);
        assert!(!link.is_open());

        link.submit(LinkCommand::Open { baud: 921_600 });

        let events = run(&mut link, &board, 50);
        assert!(matches!(events[0], LinkEvent::Opened { .. }));
        assert!(
            matches!(&events[1], LinkEvent::Line(line) if line == "[INIT] booting"),
            "{events:?}"
        );
    }

    /// Console text outside frames and the board's hello on the new link
    /// session come out of one wire, in order.
    #[test]
    fn boot_text_and_the_hello_reach_the_model_from_one_wire() {
        let (mut link, board) = link_and_board();
        board.say("ESP-ROM:esp32c6-20220919\n");

        link.submit(LinkCommand::Open { baud: 921_600 });

        let events = run(&mut link, &board, 50);
        assert!(matches!(events[0], LinkEvent::Opened { .. }));
        assert!(matches!(&events[1], LinkEvent::Line(line) if line.contains("ESP-ROM")));
        assert!(
            events.iter().any(|event| matches!(
                event,
                LinkEvent::Frame(frame) if matches!(frame.body, ServerFrameBody::Hello(_))
            )),
            "{events:?}"
        );
    }

    #[test]
    fn a_hello_request_is_answered_over_the_link() {
        let (mut link, board) = open_and_up();

        link.submit(LinkCommand::SendFrame(ClientFrame::hello(1)));

        let events = run(&mut link, &board, 50);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, LinkEvent::Error(_))),
            "{events:?}"
        );
        assert_eq!(board.request_ids(), vec![1]);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LinkEvent::Frame(frame) if frame.request_id == 1)),
            "{events:?}"
        );
    }

    /// R4b: `Reboot` is a real wire request, so it reaches the board as one
    /// instead of being refused.
    #[test]
    fn a_reboot_request_reaches_the_board() {
        let (mut link, board) = open_and_up();

        link.submit(LinkCommand::SendFrame(ClientFrame {
            request_id: 1,
            body: lpa_devices::wire::ClientFrameBody::Reboot,
        }));
        run(&mut link, &board, 20);

        assert!(
            board
                .requests_json()
                .iter()
                .any(|json| json.contains("\"reboot\"")),
            "{:?}",
            board.requests_json()
        );
    }

    #[test]
    fn a_request_the_wire_cannot_carry_is_an_error_not_a_silent_drop() {
        let (mut link, board) = open_and_up();

        link.submit(LinkCommand::SendFrame(ClientFrame {
            request_id: 1,
            body: lpa_devices::wire::ClientFrameBody::Opaque {
                label: "Flash".to_string(),
            },
        }));

        assert!(matches!(link.poll_event(), Some(LinkEvent::Error(_))));
        run(&mut link, &board, 20);
        assert!(board.request_ids().is_empty());
    }

    /// An app conversation's reply comes off the same link as the model's
    /// frames and is told apart by its id alone: it surfaces as a
    /// passthrough carrying the `M!{json}` line the conversation reads, while
    /// a model-range reply is mirrored as usual.
    #[test]
    fn an_app_range_reply_passes_through_beside_model_frames() {
        let (mut link, board) = open_and_up();
        let app_id = u64::from(lpa_devices::link::APP_CONVERSATION_ID_BASE);

        board.reply(WireServerMessage::new(app_id, ServerMsgBody::UnloadProject));
        board.reply(WireServerMessage::new(7, ServerMsgBody::UnloadProject));
        let events = run(&mut link, &board, 30);

        assert_eq!(
            events[0],
            LinkEvent::Passthrough {
                request_id: lpa_devices::link::APP_CONVERSATION_ID_BASE,
                line: "M!{\"id\":1073741824,\"msg\":\"unloadProject\"}".to_string(),
            }
        );
        assert!(matches!(&events[1], LinkEvent::Frame(frame) if frame.request_id == 7));
    }

    /// A conversation's request is its `M!{json}` line; the link carries the
    /// JSON as one message.
    #[test]
    fn a_request_line_goes_out_as_one_link_message() {
        let (mut link, board) = open_and_up();

        link.submit(LinkCommand::SendLine(
            "M!{\"id\":1073741824,\"msg\":\"hello\"}".to_string(),
        ));
        run(&mut link, &board, 20);

        assert_eq!(
            board.requests_json(),
            vec!["{\"id\":1073741824,\"msg\":\"hello\"}".to_string()]
        );
    }

    /// D9: a board that reboots under the link is a link-reset note at once
    /// (the effects layer fails shared conversations on it), then the new
    /// session's hello.
    #[test]
    fn a_board_reboot_is_a_link_reset_then_a_new_hello() {
        let (mut link, board) = open_and_up();

        board.reboot();
        let events = run(&mut link, &board, 300);

        let reset = events
            .iter()
            .position(|event| matches!(
                event,
                LinkEvent::WireNote(note) if crate::device_link::port_read_map::is_link_reset_note(note)
            ))
            .unwrap_or_else(|| panic!("the reset is surfaced: {events:?}"));
        assert!(
            events[reset..].iter().any(|event| matches!(
                event,
                LinkEvent::Frame(frame) if matches!(frame.body, ServerFrameBody::Hello(_))
            )),
            "{events:?}"
        );
    }

    /// Pause-the-pump over the link: a borrower drains the SAME link (the
    /// board's hello and its answer come to it, undecoded by the model), and
    /// the transport's own events stay queued for the model.
    #[test]
    fn a_borrower_drains_the_same_link_and_leaves_the_models_events() {
        let (mut link, board) = link_and_board();
        link.submit(LinkCommand::Open { baud: 921_600 });
        let port = link.port_handle();

        let mut ids = Vec::new();
        let drain = |ids: &mut Vec<u64>| {
            for _ in 0..50 {
                for read in port.take_reads().unwrap() {
                    if let WireRead::Frame(frame) = read {
                        ids.push(frame.message.unwrap().id);
                    }
                }
                board.tick_ms();
            }
        };
        drain(&mut ids);
        port.send(&ClientMessage {
            id: 5,
            msg: lpc_wire::ClientRequest::Hello,
        })
        .unwrap();
        drain(&mut ids);

        assert_eq!(ids, vec![0, 5], "the hello, then the answer");
        assert!(matches!(link.poll_event(), Some(LinkEvent::Opened { .. })));
    }

    #[test]
    fn a_disconnected_wire_closes_the_link_instead_of_erroring_forever() {
        let (mut link, board) = open_and_up();
        board.0.lock().unwrap().closed = true;

        let events = run(&mut link, &board, 5);
        assert!(
            matches!(&events[0], LinkEvent::Closed { reason } if reason.contains("disconnected")),
            "{events:?}"
        );
        assert!(!link.is_open());
        assert_eq!(link.poll_event(), None, "a closed link stops reading");
    }

    #[test]
    fn a_reset_writes_the_dance_and_reports_its_outcome() {
        let (mut link, board) = open_and_up();

        link.submit(LinkCommand::RunReset(ResetKind::Normal));

        let events: Vec<LinkEvent> = std::iter::from_fn(|| link.poll_event()).collect();
        assert!(
            events.iter().any(|event| matches!(
                event,
                LinkEvent::ResetOutcome {
                    kind: ResetKind::Normal,
                    ok: true
                }
            )),
            "{events:?}"
        );
        // The CH34x sequence writes both pins every step; the normal one uses
        // single-pin writes. Both must reach the wire pin-write for
        // pin-write, so the fake's edge detection sees what silicon would.
        assert_eq!(board.0.lock().unwrap().signals.len(), 3);
    }

    /// Cancelling identification is "give the port back, tell me when it is
    /// back". An unanswered close makes the model burn its whole cancel grace.
    #[test]
    fn every_close_gets_an_answer_even_on_a_link_that_never_opened() {
        let (mut link, _board) = link_and_board();

        link.submit(LinkCommand::Close);

        assert!(matches!(link.poll_event(), Some(LinkEvent::Closed { .. })));
    }

    #[test]
    fn opening_reopens_the_stream_at_the_models_baud() {
        let (mut link, board) = link_and_board();

        link.submit(LinkCommand::Open { baud: 921_600 });

        assert_eq!(board.0.lock().unwrap().reopens, vec![921_600]);
    }

    /// A link on a board double, with one clock for both ends that
    /// [`run`] advances by hand.
    fn link_and_board() -> (ByteStreamLink<BoardStream>, BoardStream) {
        let clock = Arc::new(AtomicU64::new(0));
        let board = BoardStream::new(Arc::clone(&clock));
        let link = ByteStreamLink::new(
            LinkInfo {
                label: "board double".to_string(),
                endpoint: EndpointKey("board-double".to_string()),
                usb: None,
                serial_number: None,
                carries_update_channel: false,
            },
            board.clone(),
        )
        .with_clock_us(move || clock.load(Ordering::SeqCst));
        (link, board)
    }

    /// Open, bring the link up and drain the hello.
    fn open_and_up() -> (ByteStreamLink<BoardStream>, BoardStream) {
        let (mut link, board) = link_and_board();
        link.submit(LinkCommand::Open { baud: 921_600 });
        let events = run(&mut link, &board, 50);
        assert!(
            events.iter().any(|event| matches!(
                event,
                LinkEvent::Frame(frame) if matches!(frame.body, ServerFrameBody::Hello(_))
            )),
            "the link came up: {events:?}"
        );
        (link, board)
    }

    /// Poll for `ms` milliseconds of shared time, collecting events.
    fn run(link: &mut ByteStreamLink<BoardStream>, board: &BoardStream, ms: u64) -> Vec<LinkEvent> {
        let mut events = Vec::new();
        for _ in 0..ms {
            while let Some(event) = link.poll_event() {
                events.push(event);
            }
            board.tick_ms();
        }
        events
    }

    /// A board's end of the link as a byte stream: text it was told to say,
    /// a hello on every link session, an answer to every request.
    #[derive(Clone)]
    struct BoardStream(Arc<Mutex<BoardState>>);

    struct BoardState {
        clock: Arc<AtomicU64>,
        link: lpc_wire::lp_link::Link<SelectiveRepeat>,
        nonce: u32,
        out: VecDeque<u8>,
        requests: Vec<ClientMessage>,
        signals: Vec<(Option<bool>, Option<bool>)>,
        reopens: Vec<u32>,
        closed: bool,
    }

    impl BoardStream {
        fn new(clock: Arc<AtomicU64>) -> Self {
            Self(Arc::new(Mutex::new(BoardState {
                clock,
                link: lpc_wire::lp_link::Link::new(LinkConfig::usb(), 0xB0A2_0001),
                nonce: 0xB0A2_0001,
                out: VecDeque::new(),
                requests: Vec::new(),
                signals: Vec::new(),
                reopens: Vec::new(),
                closed: false,
            })))
        }

        fn say(&self, text: &str) {
            self.0.lock().unwrap().out.extend(text.as_bytes());
        }

        fn reply(&self, message: WireServerMessage) {
            self.0.lock().unwrap().send(&message);
        }

        fn reboot(&self) {
            let mut board = self.0.lock().unwrap();
            board.nonce = board.nonce.wrapping_add(1);
            board.link = lpc_wire::lp_link::Link::new(LinkConfig::usb(), board.nonce);
            board.out.clear();
        }

        fn tick_ms(&self) {
            self.0
                .lock()
                .unwrap()
                .clock
                .fetch_add(1_000, Ordering::SeqCst);
        }

        fn request_ids(&self) -> Vec<u64> {
            let board = self.0.lock().unwrap();
            board.requests.iter().map(|request| request.id).collect()
        }

        fn requests_json(&self) -> Vec<String> {
            let board = self.0.lock().unwrap();
            board
                .requests
                .iter()
                .map(|request| lpc_wire::json::to_string(request).unwrap())
                .collect()
        }
    }

    impl BoardState {
        fn now(&self) -> u64 {
            self.clock.load(Ordering::SeqCst)
        }

        fn service(&mut self) {
            use lpc_wire::lp_link::LinkEvent as Ev;
            while let Some(event) = self.link.recv() {
                match event {
                    Ev::Up { .. } => self.send(&hello()),
                    Ev::Message {
                        channel: CH_PROTO,
                        data,
                    } => {
                        let request = lpc_wire::decode_client_payload(&data).unwrap();
                        let answer =
                            WireServerMessage::new(request.id, ServerMsgBody::UnloadProject);
                        self.requests.push(request);
                        self.send(&answer);
                    }
                    _ => {}
                }
            }
            let now = self.now();
            while let Some(frame) = self.link.poll_transmit(now) {
                let frame = frame.to_vec();
                self.out.extend(frame);
            }
        }

        fn send(&mut self, message: &WireServerMessage) {
            let json = lpc_wire::json::to_string(message).unwrap();
            self.link.send(CH_PROTO, json.as_bytes()).unwrap();
        }
    }

    impl DeviceByteStream for BoardStream {
        fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
            let mut board = self.0.lock().unwrap();
            if board.closed {
                return Err(ByteStreamError::Closed);
            }
            board.service();
            let n = buf.len().min(board.out.len());
            for (slot, byte) in buf.iter_mut().zip(board.out.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }

        fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
            let mut board = self.0.lock().unwrap();
            let now = board.now();
            board.link.on_bytes(now, bytes);
            board.service();
            Ok(())
        }

        fn set_signals(
            &mut self,
            dtr: Option<bool>,
            rts: Option<bool>,
        ) -> Result<(), ByteStreamError> {
            self.0.lock().unwrap().signals.push((dtr, rts));
            Ok(())
        }

        fn reopen(&mut self, baud_rate: u32) -> Result<(), ByteStreamError> {
            self.0.lock().unwrap().reopens.push(baud_rate);
            Ok(())
        }
    }

    fn hello() -> WireServerMessage {
        WireServerMessage::new(
            0,
            ServerMsgBody::Hello(lpc_wire::ServerHello {
                proto: lpc_wire::WIRE_PROTO_VERSION,
                build: lpc_wire::BuildFacts {
                    features: vec![],
                    package: "fw-esp32c6".to_string(),
                    version: "unknown".into(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: Default::default(),
                device_uid: None,
                pack_format: 0,
                auth: lpc_wire::HelloAuth::TRUSTED,
            }),
        )
    }
}
