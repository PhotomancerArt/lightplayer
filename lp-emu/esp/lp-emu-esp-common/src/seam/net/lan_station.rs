//! One board's station on a virtual LAN: which network it is joined to, the
//! join or scan it is waiting on, and the events it has not yet taken.
//!
//! A join and a scan each take one stated time ([`super::LanConfig`]), in
//! guest cycles; the link comes up when the join's time comes, whether or
//! not the guest has taken the event yet. Frames flow only while the link is
//! up: a board that is not joined neither sends nor hears.

use std::collections::VecDeque;

use lp_emu_core::sched::Cycles;

use crate::seam::EndpointId;

use super::virtual_access_point::{self, JoinOutcome, ScanRecord, VirtualAccessPoint};

/// What a board's station hears about, in the order it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationEvent {
    /// Joined: the link is up.
    Associated,
    /// The network refused the password.
    AuthFailed,
    /// Nothing by that name is in range.
    NotFound,
    /// The link went down without the board asking (its network left).
    LinkLost,
    /// A scan finished; its results are ready.
    ScanDone,
}

impl StationEvent {
    /// The trace word.
    pub fn word(self) -> &'static str {
        match self {
            Self::Associated => "associated",
            Self::AuthFailed => "auth-failed",
            Self::NotFound => "not-found",
            Self::LinkLost => "link-lost",
            Self::ScanDone => "scan-done",
        }
    }
}

#[derive(Clone, Debug)]
enum Pending {
    Join(JoinOutcome, String),
    Scan,
}

/// One board's station.
#[derive(Clone, Debug)]
pub struct LanStation {
    pub endpoint: EndpointId,
    pub mac: [u8; 6],
    /// The joined network's name.
    link: Option<String>,
    /// Timed work, due in order of its cycle (one join and one scan at most).
    pending: Vec<(Cycles, Pending)>,
    events: VecDeque<StationEvent>,
    scan: Vec<ScanRecord>,
}

impl LanStation {
    pub fn new(endpoint: EndpointId, mac: [u8; 6]) -> Self {
        Self {
            endpoint,
            mac,
            link: None,
            pending: Vec::new(),
            events: VecDeque::new(),
            scan: Vec::new(),
        }
    }

    /// Start joining `name`; the outcome is decided now (against the
    /// networks in range now) and lands at `due`. A board already joined
    /// leaves first, without an event. Another join in progress is replaced.
    pub fn connect(&mut self, due: Cycles, outcome: JoinOutcome, name: &[u8]) {
        self.link = None;
        self.pending
            .retain(|(_, p)| !matches!(p, Pending::Join(..)));
        let name = String::from_utf8_lossy(name).into_owned();
        self.pending.push((due, Pending::Join(outcome, name)));
    }

    /// Leave the network (and stop joining one), without an event.
    pub fn disconnect(&mut self) {
        self.link = None;
        self.pending
            .retain(|(_, p)| !matches!(p, Pending::Join(..)));
    }

    /// The board restarted (a reboot, a power cycle): its radio forgets
    /// everything — the link, a join or scan in progress, the events it had
    /// not taken and the last scan's results — with no event. Its MAC, and
    /// so its lease, stay.
    pub fn reset(&mut self) {
        self.link = None;
        self.pending.clear();
        self.events.clear();
        self.scan.clear();
    }

    /// Start a scan that finishes at `due`. One already running keeps its
    /// time.
    pub fn scan_start(&mut self, due: Cycles) {
        if !self.pending.iter().any(|(_, p)| matches!(p, Pending::Scan)) {
            self.pending.push((due, Pending::Scan));
        }
    }

    /// Land everything due at or before `now`: a join brings the link up (or
    /// queues its refusal), a scan records what is in range at its end.
    pub fn advance(&mut self, now: Cycles, access_points: &[VirtualAccessPoint]) {
        // Stable: equal times land in the order they were asked for.
        self.pending.sort_by_key(|(due, _)| *due);
        while self.pending.first().is_some_and(|(due, _)| *due <= now) {
            let (_, work) = self.pending.remove(0);
            match work {
                Pending::Join(JoinOutcome::Associated(_), name) => {
                    self.link = Some(name);
                    self.events.push_back(StationEvent::Associated);
                }
                Pending::Join(JoinOutcome::AuthFailed, _) => {
                    self.events.push_back(StationEvent::AuthFailed);
                }
                Pending::Join(JoinOutcome::NotFound, _) => {
                    self.events.push_back(StationEvent::NotFound);
                }
                Pending::Scan => {
                    self.scan = virtual_access_point::scan(access_points);
                    self.events.push_back(StationEvent::ScanDone);
                }
            }
        }
    }

    /// The network `name` left range: a board joined to it loses its link.
    pub fn network_left(&mut self, name: &str) {
        if self.link.as_deref() == Some(name) {
            self.link = None;
            self.events.push_back(StationEvent::LinkLost);
        }
    }

    pub fn link_up(&self) -> bool {
        self.link.is_some()
    }

    /// The joined network's name.
    pub fn network(&self) -> Option<&str> {
        self.link.as_deref()
    }

    pub fn take_event(&mut self) -> Option<StationEvent> {
        self.events.pop_front()
    }

    pub fn has_event(&self) -> bool {
        !self.events.is_empty()
    }

    /// The last finished scan's results, strongest first.
    pub fn scan_results(&self) -> &[ScanRecord] {
        &self.scan
    }

    /// The earliest cycle something pending lands.
    pub fn next_due(&self) -> Option<Cycles> {
        self.pending.iter().map(|(due, _)| *due).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::ParticipantId;

    #[test]
    fn a_join_lands_at_its_time_and_brings_the_link_up() {
        let aps = [VirtualAccessPoint::secured("home", "pw", -50)];
        let mut s = station();
        s.connect(
            100,
            virtual_access_point::join(&aps, b"home", b"pw"),
            b"home",
        );
        s.advance(99, &aps);
        assert!(!s.link_up());
        assert!(!s.has_event());
        s.advance(100, &aps);
        assert!(s.link_up());
        assert_eq!(s.network(), Some("home"));
        assert_eq!(s.take_event(), Some(StationEvent::Associated));
        assert_eq!(s.take_event(), None);
    }

    #[test]
    fn a_refused_join_keeps_the_link_down_and_says_why() {
        let aps = [VirtualAccessPoint::secured("home", "pw", -50)];
        let mut s = station();
        s.connect(
            10,
            virtual_access_point::join(&aps, b"home", b"no"),
            b"home",
        );
        s.connect(20, virtual_access_point::join(&aps, b"gone", b""), b"gone");
        s.advance(30, &aps);
        assert!(!s.link_up());
        assert_eq!(
            s.take_event(),
            Some(StationEvent::NotFound),
            "the second join replaced the first"
        );
        assert_eq!(s.take_event(), None);
    }

    #[test]
    fn a_scan_reports_at_its_end_and_a_lost_network_drops_the_link() {
        let aps = [
            VirtualAccessPoint::open("cafe", -80),
            VirtualAccessPoint::secured("home", "pw", -40),
        ];
        let mut s = station();
        s.scan_start(50);
        s.connect(
            10,
            virtual_access_point::join(&aps, b"home", b"pw"),
            b"home",
        );
        assert_eq!(s.next_due(), Some(10));
        s.advance(60, &aps);
        assert_eq!(s.take_event(), Some(StationEvent::Associated));
        assert_eq!(s.take_event(), Some(StationEvent::ScanDone));
        assert_eq!(s.scan_results()[0].name, "home");
        s.network_left("cafe");
        assert!(s.link_up());
        s.network_left("home");
        assert!(!s.link_up());
        assert_eq!(s.take_event(), Some(StationEvent::LinkLost));
    }

    fn station() -> LanStation {
        LanStation::new(
            EndpointId {
                board: ParticipantId(0),
                seam: "net",
            },
            [2, 0, 0, 0, 0, 0x10],
        )
    }
}
