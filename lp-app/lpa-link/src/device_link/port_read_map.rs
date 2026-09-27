//! A link port's reads ([`PortRead`]) in the shape every drainer above the
//! byte boundary already reads ([`WireRead`], [`ReadFrame`]) — plan D1: the
//! cut to lp-link is at the bytes, and the message representation above it
//! stays.
//!
//! | [`PortRead`] | becomes |
//! |---|---|
//! | `Message` | [`WireRead::Frame`] (decoded once, JSON or packed) |
//! | `Log` | [`WireRead::Line`] (a console line, as the board always printed it) |
//! | `Reset` | [`WireRead::LinkReset`] — fail what is in flight (D9) |
//! | `Up`, `Note` | a journal note ([`MappedRead::Note`]), for the model's pump only |

use lpc_wire::lp_link::ResetReason;
use lpc_wire::{PortRead, ServerPayload};

use crate::device_link::wire_reader::{ReadFrame, WireRead};

/// What every link-reset note starts with, so a reader of the journal (and
/// the effects layer, which fails shared conversations on it) knows one
/// when it sees one.
pub const LINK_RESET_NOTE_PREFIX: &str = "link: reset";

/// One [`PortRead`], sorted by who reads it.
#[derive(Debug)]
pub enum MappedRead {
    /// For whoever drains the port (the pump, or a borrower).
    Read(WireRead),
    /// For the device journal only (the model's pump).
    Note(String),
}

/// Sort one [`PortRead`]. See the module docs.
pub fn map_port_read(read: PortRead) -> MappedRead {
    match read {
        PortRead::Message(payload) => MappedRead::Read(WireRead::Frame(payload.into())),
        PortRead::Log(line) => MappedRead::Read(WireRead::Line(line)),
        PortRead::Reset { reason } => MappedRead::Read(WireRead::LinkReset(link_reset_note(reason))),
        PortRead::Up { generation } => {
            MappedRead::Note(format!("link: up (session {generation})"))
        }
        PortRead::Note(note) => MappedRead::Note(note),
    }
}

/// The journal line (and the conversation error) a link reset earns.
pub fn link_reset_note(reason: ResetReason) -> String {
    let why = match reason {
        ResetReason::PeerRestarted => "the board restarted its end",
        ResetReason::RetryLimit => "a frame went unanswered too many times",
        ResetReason::ProtocolError => "a protocol error",
        ResetReason::Requested => "this side restarted it",
    };
    format!("{LINK_RESET_NOTE_PREFIX} ({why}); requests in flight failed")
}

/// Whether a journal note is a link reset's.
pub fn is_link_reset_note(note: &str) -> bool {
    note.starts_with(LINK_RESET_NOTE_PREFIX)
}

impl From<ServerPayload> for ReadFrame {
    fn from(payload: ServerPayload) -> Self {
        ReadFrame {
            json: payload.json,
            packed: payload.packed,
            message: payload.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_is_a_read_every_drainer_sees() {
        let MappedRead::Read(WireRead::LinkReset(note)) = map_port_read(PortRead::Reset {
            reason: ResetReason::PeerRestarted,
        }) else {
            panic!("a reset is a read");
        };
        assert!(is_link_reset_note(&note), "{note}");
        assert!(note.contains("board restarted"), "{note}");
    }

    #[test]
    fn up_and_notes_are_for_the_journal() {
        assert!(matches!(
            map_port_read(PortRead::Up { generation: 3 }),
            MappedRead::Note(note) if note.contains("session 3")
        ));
        assert!(matches!(
            map_port_read(PortRead::Note("wire: x".into())),
            MappedRead::Note(note) if note == "wire: x"
        ));
        assert!(!is_link_reset_note("link: up (session 1)"));
    }

    #[test]
    fn a_log_line_is_a_console_line() {
        assert!(matches!(
            map_port_read(PortRead::Log("[INFO] boot".into())),
            MappedRead::Read(WireRead::Line(line)) if line == "[INFO] boot"
        ));
    }
}
