//! [`Link`] over an emulated board hosted in this tab.
//!
//! The board runs the shipped `fw-esp32c6` image, whose USB link is an
//! lp-link, so this link does what the Web Serial one does: it opens and
//! closes the board's byte channel, queues requests on the board's lp-link
//! end (`emulator_tab_link_port`) and demuxes what that end decoded. The
//! bytes themselves are the link port's loop's business, not this type's.
//!
//! Commands are synchronous here — the bridge queues every verb on the
//! board's one chain in the page — so, like the old `ByteStreamLink` this
//! replaces for the tab, [`Link::poll_event`] needs no future at all.
//!
//! # Why the reset dance needs no delays
//!
//! [`ResetKind`]'s pin sequences are written with **no** inter-step holds,
//! because this may not block the caller. That is right for the emulator:
//! it decodes the reset from the pin EDGES rather than pattern-matching a
//! timed sequence (`lp-emu/esp/README.md`, "decoded, not pattern-matched"),
//! so the ROM download dance lands whatever the wall-clock gaps were. Real
//! silicon is what needs the ~100 ms holds, and its controller has them
//! (`browser_esp32_device_controller.js`'s `runReset`).
//!
//! # Baud is not a thing here
//!
//! The board's link is USB-Serial-JTAG: there is no line rate to set, and
//! the machine ignores one. `Open` takes the model's baud and drops it rather
//! than pretending a number was applied.

use std::collections::VecDeque;

use lpa_devices::link::{Link, LinkCommand, LinkEvent, LinkInfo, ResetKind};

use super::emulator_tab_bridge::EmulatorTabPort;
use super::emulator_tab_link_port::{
    close_link, open_link, send_client_json, take_notes, take_reads,
};
use crate::device_link::demux::demux_read;
use crate::device_link::wire::client_message;

/// One link to a tab-hosted board. Attached but closed until the model sends
/// `LinkCommand::Open`.
pub struct EmulatorTabLink {
    info: LinkInfo,
    port: EmulatorTabPort,
    open: bool,
    events: VecDeque<LinkEvent>,
}

impl EmulatorTabLink {
    pub fn new(info: LinkInfo, port: EmulatorTabPort) -> Self {
        Self {
            info,
            port,
            open: false,
            events: VecDeque::new(),
        }
    }

    /// Whether the board's byte channel is open for traffic.
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn open_port(&mut self) {
        match self.port.reopen() {
            Ok(()) => {
                // A fresh session (a new nonce): the board starts over with
                // it, and says hello when the link is up.
                open_link(self.port);
                self.open = true;
                self.events.push_back(LinkEvent::Opened {
                    info: self.info.clone(),
                });
            }
            Err(error) => self.events.push_back(LinkEvent::Error(error.to_string())),
        }
    }

    /// Close the byte channel and SAY so, even if it was not open: every
    /// `Close` gets an answer, or the model waits out its cancel grace.
    fn close_port(&mut self, reason: &str) {
        close_link(self.port);
        if self.open
            && let Err(error) = self.port.close()
        {
            self.events.push_back(LinkEvent::Error(error.to_string()));
        }
        self.open = false;
        self.events.push_back(LinkEvent::Closed {
            reason: reason.to_string(),
        });
    }

    /// Run one reset dance as single-pin writes (see the module docs).
    fn run_reset(&mut self, kind: ResetKind) {
        for (dtr, rts) in reset_steps(kind) {
            if let Err(error) = self.port.signals(*dtr, *rts) {
                self.events.push_back(LinkEvent::Error(error.to_string()));
                self.events
                    .push_back(LinkEvent::ResetOutcome { kind, ok: false });
                return;
            }
        }
        // The board reboots: its end of the link comes back with a new
        // nonce, and this end hears that as a reset and then a new session.
        self.events
            .push_back(LinkEvent::ResetOutcome { kind, ok: true });
    }

    fn send_json(&mut self, json: &str) {
        if !self.open {
            self.events.push_back(LinkEvent::Error(
                "write on a link that is not open".to_string(),
            ));
            return;
        }
        if let Err(error) = send_client_json(self.port, json) {
            self.events.push_back(LinkEvent::Error(error));
        }
    }

    /// What the page's work queue failed at, then what the board's link
    /// decoded, onto the event queue.
    fn pump(&mut self) {
        // A failure on the page's work queue is the link's error event (a
        // write that could not be applied, an open that was refused),
        // reported before the reads so the model hears it in order.
        if let Some(error) = self.port.take_error() {
            self.events.push_back(LinkEvent::Error(error));
        }
        if !self.open {
            return;
        }
        let Some(reads) = take_reads(self.port) else {
            // The handle is gone (`dispose`): a closed port, not an error
            // for the model to narrate.
            self.open = false;
            self.events.push_back(LinkEvent::Closed {
                reason: "device disconnected".to_string(),
            });
            return;
        };
        for read in reads {
            self.events.push_back(demux_read(read));
        }
        for note in take_notes(self.port) {
            self.events.push_back(LinkEvent::WireNote(note));
        }
    }
}

impl Link for EmulatorTabLink {
    fn info(&self) -> &LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: LinkCommand) {
        match command {
            LinkCommand::Open { .. } => self.open_port(),
            LinkCommand::Close => self.close_port("closed by request"),
            LinkCommand::RunReset(kind) => self.run_reset(kind),
            LinkCommand::SendFrame(frame) => match client_message(&frame).and_then(|message| {
                lpc_wire::json::to_string(&message).map_err(|error| error.to_string())
            }) {
                Ok(json) => self.send_json(&json),
                Err(error) => self.events.push_back(LinkEvent::Error(error)),
            },
            // A conversation's request, spelled as the line it was.
            LinkCommand::SendLine(line) => match line.trim_end().strip_prefix("M!") {
                Some(json) => self.send_json(json),
                None => self.events.push_back(LinkEvent::Error(format!(
                    "not a request, and the link carries no raw text to the board: {line:?}"
                ))),
            },
        }
    }

    fn poll_event(&mut self) -> Option<LinkEvent> {
        if self.events.is_empty() {
            self.pump();
        }
        self.events.pop_front()
    }
}

/// The pin writes each reset kind is, as `(DTR, RTS)` with `None` for "leave
/// it". The same sequences as the controller's `runReset` and the host
/// `ByteStreamLink`'s table, minus the holds (see the module docs).
fn reset_steps(kind: ResetKind) -> &'static [(Option<bool>, Option<bool>)] {
    match kind {
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
        // The CH34x sequence: whole-status writes, never crossing (DTR
        // asserted, RTS released).
        ResetKind::BothThenDrop => &[
            (Some(false), Some(false)),
            (Some(true), Some(true)),
            (Some(false), Some(true)),
            (Some(false), Some(false)),
        ],
    }
}
