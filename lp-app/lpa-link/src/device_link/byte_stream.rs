//! [`Link`] over the [`DeviceByteStream`] seam: commands in, events out, no
//! executor.
//!
//! This is the host-side half of M3's dependency inversion. The seam it wraps
//! is the same one the real serial transport drives
//! (`lpa_client::transport_serial::hardware`), so the fake device, a native
//! port, and anything else that can be a byte pipe all reach the model
//! through one adapter.
//!
//! # Why this can be synchronous
//!
//! [`DeviceByteStream`] is deliberately sync: `read_available` returns `Ok(0)`
//! rather than waiting. That is exactly the shape [`Link::poll_event`] wants
//! — "take the next event, if one is ready, never block" — so this adapter
//! needs no runtime at all. The browser adapter cannot do this (Web Serial is
//! promise-shaped) and spawns futures instead; see `browser_serial`.
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

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, ResetKind};

use crate::device_link::demux::{LineSplitter, push_bytes};
use crate::device_link::wire::encode_client_frame;
use crate::stream::{ByteStreamError, DeviceByteStream};

/// Bytes read per `read_available` call.
const READ_CHUNK: usize = 4096;

/// Reads per pump. A firehose must not hold the caller: whatever is left is
/// read on the next [`Link::poll_event`], and the model is event-driven
/// anyway.
const READS_PER_PUMP: usize = 64;

/// One open (or opening) link over a byte stream.
pub struct ByteStreamLink<S: DeviceByteStream> {
    info: LinkInfo,
    stream: S,
    open: bool,
    splitter: LineSplitter,
    events: VecDeque<LinkEvent>,
}

impl<S: DeviceByteStream> ByteStreamLink<S> {
    /// A link that is attached but not yet open. Nothing happens on the wire
    /// until the model sends `LinkCommand::Open`.
    pub fn new(info: LinkInfo, stream: S) -> Self {
        Self {
            info,
            stream,
            open: false,
            splitter: LineSplitter::default(),
            events: VecDeque::new(),
        }
    }

    /// Whether the port is currently open for traffic.
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn open_port(&mut self, baud: u32) {
        // A fresh port is a fresh window (the fold begins one on `Opened`),
        // so the previous generation's partial line is not this one's first.
        self.splitter.clear();
        match self.stream.reopen(baud) {
            Ok(()) => {
                self.open = true;
                self.events.push_back(LinkEvent::Opened {
                    info: self.info.clone(),
                });
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
        self.open = false;
        self.splitter.clear();
        self.events.push_back(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    fn write(&mut self, bytes: &[u8]) {
        if !self.open {
            self.events.push_back(LinkEvent::Error(
                "write on a link that is not open".to_string(),
            ));
            return;
        }
        if let Err(error) = self.stream.write_all(bytes) {
            self.fail(&error);
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
                self.events
                    .push_back(LinkEvent::ResetOutcome { kind, ok: false });
                return;
            }
        }
        // A device that just rebooted is re-describing itself from scratch,
        // so anything half-read belongs to the machine that no longer exists.
        self.splitter.clear();
        self.events
            .push_back(LinkEvent::ResetOutcome { kind, ok: true });
    }

    /// Read whatever the wire has and demux it onto the event queue.
    fn pump(&mut self) {
        if !self.open {
            return;
        }
        let mut buf = [0u8; READ_CHUNK];
        for _ in 0..READS_PER_PUMP {
            match self.stream.read_available(&mut buf) {
                Ok(0) => return,
                Ok(read) => {
                    push_bytes(&mut self.splitter, &buf[..read], &mut self.events);
                }
                Err(ByteStreamError::Closed) => {
                    self.close_port("device disconnected");
                    return;
                }
                Err(error) => {
                    self.fail(&error);
                    return;
                }
            }
        }
    }

    /// Surface an IO failure as an event. Never a return value: the model is
    /// not where IO errors are decided.
    fn fail(&mut self, error: &ByteStreamError) {
        self.events.push_back(LinkEvent::Error(error.to_string()));
    }
}

impl<S: DeviceByteStream> Link for ByteStreamLink<S> {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        match command {
            LinkCommand::Open { baud } => self.open_port(baud),
            LinkCommand::Close => self.close_port("closed by request"),
            LinkCommand::RunReset(kind) => self.run_reset(kind),
            LinkCommand::SendFrame(frame) => match encode_client_frame(&frame) {
                Ok(line) => self.write(line.as_bytes()),
                Err(error) => self.events.push_back(LinkEvent::Error(error)),
            },
            LinkCommand::SendLine(line) => self.write(format!("{line}\n").as_bytes()),
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.events.is_empty() {
            self.pump();
        }
        self.events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::identity::EndpointKey;
    use lpa_devices::wire::{ClientFrame, ServerFrameBody};

    /// A byte pipe a test can script: queued device output, recorded writes
    /// and pin edges.
    #[derive(Default)]
    struct ScriptedStream {
        out: VecDeque<u8>,
        written: Vec<u8>,
        signals: Vec<(Option<bool>, Option<bool>)>,
        reopens: Vec<u32>,
        closed: bool,
    }

    impl ScriptedStream {
        fn say(&mut self, text: &str) {
            self.out.extend(text.as_bytes());
        }

        fn written_text(&self) -> String {
            String::from_utf8_lossy(&self.written).into_owned()
        }
    }

    impl DeviceByteStream for ScriptedStream {
        fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
            if self.closed {
                return Err(ByteStreamError::Closed);
            }
            let mut written = 0;
            while written < buf.len() {
                let Some(byte) = self.out.pop_front() else {
                    break;
                };
                buf[written] = byte;
                written += 1;
            }
            Ok(written)
        }

        fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
            self.written.extend_from_slice(bytes);
            Ok(())
        }

        fn set_signals(
            &mut self,
            dtr: Option<bool>,
            rts: Option<bool>,
        ) -> Result<(), ByteStreamError> {
            self.signals.push((dtr, rts));
            Ok(())
        }

        fn reopen(&mut self, baud_rate: u32) -> Result<(), ByteStreamError> {
            self.reopens.push(baud_rate);
            Ok(())
        }
    }

    fn link(stream: ScriptedStream) -> ByteStreamLink<ScriptedStream> {
        ByteStreamLink::new(
            LinkInfo {
                label: "scripted".to_string(),
                endpoint: EndpointKey("scripted".to_string()),
                usb: None,
                serial_number: None,
            },
            stream,
        )
    }

    /// A closed link is silent: nothing is read before the model opens the
    /// port, so a grant that is merely held produces no evidence.
    #[test]
    fn nothing_flows_until_the_model_opens_the_port() {
        let mut stream = ScriptedStream::default();
        stream.say("[INIT] booting\n");
        let mut link = link(stream);

        assert_eq!(link.poll_event(), None);
        assert!(!link.is_open());

        link.submit(LinkCommand::Open { baud: 921_600 });

        assert!(matches!(link.poll_event(), Some(LinkEvent::Opened { .. })));
        assert!(matches!(
            link.poll_event(),
            Some(LinkEvent::Line(line)) if line == "[INIT] booting"
        ));
        assert_eq!(link.poll_event(), None);
    }

    #[test]
    fn a_hello_request_goes_out_as_the_line_a_device_answers() {
        let mut link = link(ScriptedStream::default());
        link.submit(LinkCommand::Open { baud: 921_600 });

        link.submit(LinkCommand::SendFrame(ClientFrame::hello(1)));

        // No error event: the write landed.
        while let Some(event) = link.poll_event() {
            assert!(!matches!(event, LinkEvent::Error(_)), "{event:?}");
        }
    }

    #[test]
    fn frames_and_lines_reach_the_model_from_one_wire() {
        let mut stream = ScriptedStream::default();
        stream.say("ESP-ROM:esp32c6-20220919\nM!{\"id\":0,\"msg\":\"unloadProject\"}\n");
        let mut link = link(stream);

        link.submit(LinkCommand::Open { baud: 921_600 });

        let events: Vec<LinkEvent> = std::iter::from_fn(|| link.poll_event()).collect();
        assert!(matches!(events[0], LinkEvent::Opened { .. }));
        assert!(matches!(&events[1], LinkEvent::Line(line) if line.contains("ESP-ROM")));
        assert!(matches!(
            &events[2],
            LinkEvent::Frame(frame) if matches!(frame.body, ServerFrameBody::Other { .. })
        ));
    }

    #[test]
    fn a_disconnected_wire_closes_the_link_instead_of_erroring_forever() {
        let mut link = link(ScriptedStream::default());
        link.submit(LinkCommand::Open { baud: 921_600 });
        assert!(matches!(link.poll_event(), Some(LinkEvent::Opened { .. })));
        link.stream.closed = true;

        assert!(matches!(
            link.poll_event(),
            Some(LinkEvent::Closed { reason }) if reason.contains("disconnected")
        ));
        assert!(!link.is_open());
        assert_eq!(link.poll_event(), None, "a closed link stops reading");
    }

    #[test]
    fn a_reset_writes_the_dance_and_reports_its_outcome() {
        let mut link = link(ScriptedStream::default());
        link.submit(LinkCommand::Open { baud: 921_600 });

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
        assert_eq!(link.stream.signals.len(), 3);
    }

    #[test]
    fn a_request_the_wire_cannot_carry_is_an_error_not_a_silent_drop() {
        let mut link = link(ScriptedStream::default());
        link.submit(LinkCommand::Open { baud: 921_600 });
        while link.poll_event().is_some() {}

        link.submit(LinkCommand::SendFrame(ClientFrame {
            request_id: 1,
            body: lpa_devices::wire::ClientFrameBody::Reboot,
        }));

        assert!(matches!(link.poll_event(), Some(LinkEvent::Error(_))));
        assert!(link.stream.written_text().is_empty());
    }

    /// Cancelling identification is "give the port back, tell me when it is
    /// back". An unanswered close makes the model burn its whole cancel grace.
    #[test]
    fn every_close_gets_an_answer_even_on_a_link_that_never_opened() {
        let mut link = link(ScriptedStream::default());

        link.submit(LinkCommand::Close);

        assert!(matches!(link.poll_event(), Some(LinkEvent::Closed { .. })));
    }

    #[test]
    fn opening_reopens_the_stream_at_the_models_baud() {
        let mut link = link(ScriptedStream::default());

        link.submit(LinkCommand::Open { baud: 921_600 });

        assert_eq!(link.stream.reopens, vec![921_600]);
    }
}
