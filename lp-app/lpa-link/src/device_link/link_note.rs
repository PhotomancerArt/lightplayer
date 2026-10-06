//! The link's own journal notes, named, so a reader above the byte boundary
//! can tell a link going quiet or restarting from any other note without
//! matching free text in two places.
//!
//! The notes are minted here and in [`port_read_map`](super::port_read_map)
//! (the reset line), and they reach the app as `LinkEvent::WireNote`s — from
//! the model's pump, or from the editor lens's tap while it holds the wire.
//! Studio's "Reconnecting…" state (plan D13) reads them through
//! [`classify_link_note`].

use crate::device_link::port_read_map::LINK_RESET_NOTE_PREFIX;

/// What every link-up note starts with (`"link: up (session N)"`).
pub const LINK_UP_NOTE_PREFIX: &str = "link: up";

/// The note for a stall's leading edge: the link is up, but the board has
/// said nothing for the link's stall time.
pub const LINK_STALLED_NOTE: &str = "link: stalled — the board has gone quiet; holding the session";

/// The note for a stall's trailing edge.
pub const LINK_ANSWERING_NOTE: &str = "link: the board is answering again";

/// The note for an update message refused because the board has not
/// announced lp-link's update channel this session (DS9): sending it anyway
/// would stall the link on a frame a board without the channel never
/// acknowledges.
pub const UPDATE_NOT_ANNOUNCED_NOTE: &str =
    "link: update message not sent — the board has not announced the update channel";

/// A link-state note, by kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkNote {
    /// The link (re)established a session. The board's hello follows.
    Up,
    /// The board went quiet on an established link.
    Stalled,
    /// The board is heard again after a stall.
    Answering,
    /// The session reset; requests in flight failed (D9).
    Reset,
}

/// The kind of a journal note, when it is one of the link's own.
pub fn classify_link_note(note: &str) -> Option<LinkNote> {
    if note.starts_with(LINK_RESET_NOTE_PREFIX) {
        Some(LinkNote::Reset)
    } else if note.starts_with(LINK_UP_NOTE_PREFIX) {
        Some(LinkNote::Up)
    } else if note == LINK_STALLED_NOTE {
        Some(LinkNote::Stalled)
    } else if note == LINK_ANSWERING_NOTE {
        Some(LinkNote::Answering)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_link::port_read_map::{MappedRead, link_reset_note, map_port_read};
    use lpc_wire::PortRead;
    use lpc_wire::lp_link::ResetReason;

    #[test]
    fn the_link_notes_are_named() {
        assert_eq!(
            classify_link_note(&link_reset_note(ResetReason::RetryLimit)),
            Some(LinkNote::Reset)
        );
        let MappedRead::Note(up) = map_port_read(PortRead::Up { generation: 2 }) else {
            panic!("up is a note");
        };
        assert_eq!(classify_link_note(&up), Some(LinkNote::Up));
        assert_eq!(
            classify_link_note(LINK_STALLED_NOTE),
            Some(LinkNote::Stalled)
        );
        assert_eq!(
            classify_link_note(LINK_ANSWERING_NOTE),
            Some(LinkNote::Answering)
        );
    }

    #[test]
    fn other_notes_are_not_the_links() {
        assert_eq!(classify_link_note("wire: replies packed"), None);
        assert_eq!(classify_link_note("link: a reply did not decode"), None);
    }
}
