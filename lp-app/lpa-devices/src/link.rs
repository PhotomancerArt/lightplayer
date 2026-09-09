//! The transport contract, defined **here** and implemented by transports
//! (vision R2, dependency inversion).
//!
//! A link is dumb: commands in, events out. It does no classification — the
//! hello gate, boot-line diagnosis and foreign-firmware detection all live
//! in the device fold, which is what makes verdicts naturally non-sticky.
//! Implementations in M3 and later: browser Web Serial, host serial, the
//! M9 fake, and eventually the sim.

use serde::{Deserialize, Serialize};

use crate::identity::EndpointKey;
use crate::wire::{ClientFrame, ServerFrame};

/// Request ids at or above this base belong to **app conversations**: an
/// `lpa-client` exchange the effects layer runs on the shared link beside
/// the model's own frames (the device card's frame feed is the first). The
/// transport classifies a reply by its id BEFORE the mirror — one at or
/// above the base surfaces as [`LinkEvent::Passthrough`], verbatim, for the
/// conversation that asked — and the model never folds it. Model-minted ids
/// (the identify hello re-ask, the two `ClearFaults` constants) stay far
/// below; a conversation mints upward from here.
///
/// Why a range and not a second wire: frames are not evidence. A picture
/// stream at ~5 fps through a fold that journals every input and lines
/// every unknown frame on the terminal would drown both, and the model has
/// no use for the bytes — what a board is RUNNING reaches it on the
/// heartbeat regardless.
pub const APP_CONVERSATION_ID_BASE: u32 = 0x4000_0000;

/// Handle for one open (or opening) transport. Minted by the effects layer,
/// meaningless to the model beyond routing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct LinkId(pub u64);

/// What the effects layer knows about a transport without talking to it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkInfo {
    /// Human label for the port ("/dev/cu.usbmodem2101", "COM7").
    pub label: String,
    /// Stable fingerprint used as the weakest identity binding.
    pub endpoint: EndpointKey,
    pub usb: Option<UsbIds>,
    /// USB serial number, which on Espressif native-USB boards is the MAC —
    /// free identity before a single byte is read.
    pub serial_number: Option<String>,
}

/// USB vendor/product pair, for board guessing and grant revocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UsbIds {
    pub vendor: u16,
    pub product: u16,
}

/// Reset sequences a transport can run. Names mirror the shipped
/// `runReset` kinds plus the bench-proven CH34x fallback.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResetKind {
    /// DTR/RTS pulse: the normal ESP32 reset.
    Normal,
    RtsOnly,
    UsbJtagDownload,
    /// DTR+RTS asserted together, then dropped — the CH34x sequence that
    /// works where `Normal` does not.
    BothThenDrop,
}

/// Everything a transport can tell the model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LinkEvent {
    Opened {
        info: LinkInfo,
    },
    Closed {
        reason: String,
    },
    Frame(ServerFrame),
    /// One non-protocol serial line (boot output, logs).
    Line(String),
    ResetOutcome {
        kind: ResetKind,
        ok: bool,
    },
    Error(String),
    /// A reply to an app conversation (`request_id` ≥
    /// [`APP_CONVERSATION_ID_BASE`]): the raw `M!` line, verbatim, so the
    /// conversation can decode it with the full wire vocabulary. The
    /// effects layer routes it to whoever asked; the fold ignores it
    /// entirely — no terminal line, no freshness, no anomaly count.
    Passthrough {
        request_id: u32,
        line: String,
    },
}

/// Everything the model can ask a transport to do.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LinkCommand {
    Open { baud: u32 },
    Close,
    RunReset(ResetKind),
    SendFrame(ClientFrame),
    SendLine(String),
}

/// The one thing a transport implements. Event-queue shaped on purpose: no
/// futures, no callbacks, nothing for the model to await.
///
/// The effects layer owns the implementation, pumps [`Self::poll_event`]
/// into [`Event::Link`](crate::Event::Link), and applies
/// [`Command::Link`](crate::Command::Link) via [`Self::submit`].
pub trait Link {
    /// Static facts about the endpoint.
    fn info(&self) -> &LinkInfo;

    /// Queue one command. Failures surface later as
    /// [`LinkEvent::Error`] — never as a return value the model must handle
    /// inline, because the model is not where IO errors are decided.
    fn submit(&mut self, command: LinkCommand);

    /// Take the next event, if one is ready. Never blocks.
    fn poll_event(&mut self) -> Option<LinkEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trait is the M3 contract; this stub proves it is implementable
    /// with no executor, no IO and no borrow gymnastics.
    #[derive(Default)]
    struct StubLink {
        info: LinkInfo,
        submitted: Vec<LinkCommand>,
        pending: Vec<LinkEvent>,
    }

    impl Link for StubLink {
        fn info(&self) -> &LinkInfo {
            &self.info
        }

        fn submit(&mut self, command: LinkCommand) {
            self.submitted.push(command);
        }

        fn poll_event(&mut self) -> Option<LinkEvent> {
            if self.pending.is_empty() {
                return None;
            }
            Some(self.pending.remove(0))
        }
    }

    #[test]
    fn the_contract_is_implementable_without_io() {
        let mut link = StubLink {
            pending: vec![LinkEvent::Line("ESP-ROM:esp32c6".to_string())],
            ..Default::default()
        };

        link.submit(LinkCommand::Open { baud: 921_600 });

        assert_eq!(link.submitted.len(), 1);
        assert!(matches!(link.poll_event(), Some(LinkEvent::Line(_))));
        assert!(link.poll_event().is_none());
    }
}
