//! Whether a device's link is in trouble it is expected to come back from:
//! the board went quiet on an established lp-link session (a stall), or the
//! session reset and the board has not been heard since (plan D13).
//!
//! Read off what already reaches the fold — the link's journal notes
//! ([`classify_link_note`]) and the board's frames — so it needs no clock
//! and no transport seam of its own: every `Input::Event(Event::Link {..})`
//! the controller folds passes through [`LinkHealthMap::observe`] first, from
//! the pump and from the editor lens's tap alike.
//!
//! This is NOT a loss. A port that is gone closes the link (the model's
//! `Closed`/`LinkDetached` evidence), and the editor holds on for the board
//! to come back on a new link (`studio::lens_hold`); health only covers a
//! link whose port is still there.

use std::collections::BTreeMap;

use lpa_devices::link::LinkEvent;
use lpa_link::device_link::link_note::{LinkNote, classify_link_note};

use crate::{DeviceInput, DeviceLinkId as LinkId};

/// What is wrong with a link that is expected to recover.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTrouble {
    /// Up, but the board has said nothing for the link's stall time.
    Quiet,
    /// The session reset; the board has not been heard since.
    Restarted,
}

/// One link's health, folded from its notes and frames.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinkHealth {
    stalled: bool,
    awaiting_board: bool,
    /// Bumped each time trouble begins, so a reader can tell one episode
    /// from the next.
    episode: u32,
}

impl LinkHealth {
    /// The trouble the link is in, if any. A reset outranks a stall: the
    /// session the stall was about is gone.
    pub fn trouble(&self) -> Option<LinkTrouble> {
        if self.awaiting_board {
            Some(LinkTrouble::Restarted)
        } else if self.stalled {
            Some(LinkTrouble::Quiet)
        } else {
            None
        }
    }

    /// Which trouble episode this is (see the field).
    pub fn episode(&self) -> u32 {
        self.episode
    }

    /// Fold one link event.
    ///
    /// Any whole frame from the board ends the trouble: after a reset the
    /// first one is its hello (D5 — the board says hello first on every
    /// `Up`), and after a stall it is the board answering. A port opening or
    /// closing ends it too — a fresh port is a fresh start, and a closed one
    /// is a loss, which is not this state's to show.
    pub fn observe(&mut self, event: &LinkEvent) {
        let before = self.trouble().is_some();
        match event {
            LinkEvent::WireNote(note) => match classify_link_note(note) {
                Some(LinkNote::Stalled) => self.stalled = true,
                Some(LinkNote::Answering) => self.stalled = false,
                Some(LinkNote::Reset) => {
                    self.awaiting_board = true;
                    self.stalled = false;
                }
                Some(LinkNote::Up) | None => {}
            },
            LinkEvent::Frame(_)
            | LinkEvent::Passthrough { .. }
            | LinkEvent::Opened { .. }
            | LinkEvent::Closed { .. } => {
                self.stalled = false;
                self.awaiting_board = false;
            }
            _ => {}
        }
        if !before && self.trouble().is_some() {
            self.episode = self.episode.wrapping_add(1);
        }
    }
}

/// Every link's health, by link.
#[derive(Clone, Debug, Default)]
pub struct LinkHealthMap {
    links: BTreeMap<LinkId, LinkHealth>,
}

impl LinkHealthMap {
    /// Fold one model input; anything that is not a link event is ignored.
    pub fn observe(&mut self, input: &DeviceInput) {
        let lpa_devices::Input::Event(lpa_devices::Event::Link { link, event }) = input else {
            return;
        };
        self.links.entry(*link).or_default().observe(event);
    }

    /// One link's health (a link never heard from is healthy).
    pub fn get(&self, link: LinkId) -> LinkHealth {
        self.links.get(&link).copied().unwrap_or_default()
    }

    /// Forget links the model has let go of.
    pub fn retain(&mut self, keep: impl Fn(LinkId) -> bool) {
        self.links.retain(|link, _| keep(*link));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::ServerFrame;
    use lpa_link::device_link::link_note::{LINK_ANSWERING_NOTE, LINK_STALLED_NOTE};
    use lpa_link::device_link::port_read_map::link_reset_note;
    use lpc_wire::lp_link::ResetReason;

    #[test]
    fn a_stall_is_quiet_until_the_board_answers() {
        let mut health = LinkHealth::default();
        health.observe(&note(LINK_STALLED_NOTE));
        assert_eq!(health.trouble(), Some(LinkTrouble::Quiet));
        health.observe(&note(LINK_ANSWERING_NOTE));
        assert_eq!(health.trouble(), None);
    }

    #[test]
    fn a_reset_holds_until_the_board_is_heard_not_merely_until_the_link_is_up() {
        let mut health = LinkHealth::default();
        health.observe(&note(LINK_STALLED_NOTE));
        health.observe(&note(&link_reset_note(ResetReason::RetryLimit)));
        assert_eq!(health.trouble(), Some(LinkTrouble::Restarted));
        // The stall's trailing edge rides the reset (the session it was
        // about is gone); it must not end the wait for the board.
        health.observe(&note(LINK_ANSWERING_NOTE));
        health.observe(&note("link: up (session 2)"));
        assert_eq!(health.trouble(), Some(LinkTrouble::Restarted));
        health.observe(&LinkEvent::Frame(ServerFrame::heartbeat(None)));
        assert_eq!(health.trouble(), None);
    }

    #[test]
    fn each_new_trouble_is_a_new_episode() {
        let mut health = LinkHealth::default();
        health.observe(&note(LINK_STALLED_NOTE));
        let first = health.episode();
        // Worsening within one episode is still that episode.
        health.observe(&note(&link_reset_note(ResetReason::PeerRestarted)));
        assert_eq!(health.episode(), first);
        health.observe(&LinkEvent::Frame(ServerFrame::heartbeat(None)));
        health.observe(&note(LINK_STALLED_NOTE));
        assert_ne!(health.episode(), first);
    }

    #[test]
    fn a_closed_port_is_not_reconnecting() {
        let mut health = LinkHealth::default();
        health.observe(&note(&link_reset_note(ResetReason::RetryLimit)));
        health.observe(&LinkEvent::Closed {
            reason: "The device has been lost".to_string(),
        });
        assert_eq!(health.trouble(), None);
    }

    #[test]
    fn the_map_reads_only_link_events_and_forgets_released_links() {
        let mut map = LinkHealthMap::default();
        let link = LinkId(4);
        map.observe(&DeviceInput::link(link, note(LINK_STALLED_NOTE)));
        assert_eq!(map.get(link).trouble(), Some(LinkTrouble::Quiet));
        assert_eq!(map.get(LinkId(5)).trouble(), None);
        map.retain(|kept| kept != link);
        assert_eq!(map.get(link).trouble(), None);
    }

    fn note(text: &str) -> LinkEvent {
        LinkEvent::WireNote(text.to_string())
    }
}
