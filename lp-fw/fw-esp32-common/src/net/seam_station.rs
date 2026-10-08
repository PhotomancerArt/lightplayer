//! [`StationControl`] over the network seam (`net=lan`; plan P10): the
//! station's controls answered by the emulator's virtual access points.
//!
//! Built at network bring-up only when the seam's engaged byte reads 1
//! (`crate::seams::net::net_mac`); on silicon nothing constructs it. It keeps
//! the radio implementation's limits (`fw-esp32c6`'s `esp_station`): a scan
//! gets [`SCAN_LIMIT`], a connect [`CONNECT_LIMIT`]. A disconnect needs no
//! limit here: the seam's `net_disconnect` takes the link down before it
//! returns, so there is nothing to wait for.
//!
//! Every outcome arrives as a station event (`lp_seam::net::EVENT_*`), taken
//! one at a time with `net_event_take` after the wake
//! ([`crate::seams::seam_wake::NET_EVENTS`]) says one is waiting. What an
//! event means depends on what the station is waiting for ([`sort_event`]);
//! a `link lost` that arrives while it waits for something else is kept for
//! the next [`StationControl::wait_link_lost`].
//!
//! The password goes into `net_connect`'s buffer and nowhere else: it is
//! never logged.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::future::poll_fn;
use core::task::Poll;

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use lp_seam::net::{
    EVENT_ASSOCIATED, EVENT_AUTH_FAILED, EVENT_LINK_LOST, EVENT_NONE, EVENT_NOT_FOUND,
    EVENT_SCAN_DONE, MAX_PASSWORD_LEN, MAX_SSID_LEN, scan_record_len,
};
use lpc_wire::HeardNetwork;

use crate::net::{ConnectOutcome, StationControl};
use crate::seams::net::{
    net_connect, net_disconnect, net_event_take, net_link, net_scan_start, net_scan_take,
};
use crate::seams::seam_wake::{NET_EVENTS, NET_FRAMES};

/// How long a scan may take before it is given up (the radio's limit).
pub const SCAN_LIMIT: Duration = Duration::from_secs(6);
/// How long one connect may take to be decided (the radio's limit).
pub const CONNECT_LIMIT: Duration = Duration::from_secs(14);
/// The most networks one scan answer carries: what the buffer holds at the
/// longest names. The emulator writes whole records, strongest first, so a
/// longer list loses its weakest.
pub const SCAN_MAX_NETWORKS: usize = 32;

/// [`StationControl`] over the network seam.
pub struct SeamStation {
    /// A `link lost` taken while waiting for something else.
    link_lost: bool,
    /// The last scan's networks, for the joined one's signal.
    last_scan: Vec<HeardNetwork>,
    /// The joined network's signal, from the last scan that heard it.
    joined_rssi: Option<i8>,
}

impl SeamStation {
    pub fn new() -> Self {
        Self {
            link_lost: false,
            last_scan: Vec::new(),
            joined_rssi: None,
        }
    }

    /// Wait for the event that ends `awaiting`, up to `limit`. `None`: the
    /// limit passed first.
    async fn wait_for(&mut self, awaiting: Awaiting, limit: Option<Duration>) -> Option<Finish> {
        let link_lost = &mut self.link_lost;
        let finished = poll_fn(|cx| {
            // Register before the take, so a wake between the two is not lost.
            NET_EVENTS.register(cx.waker());
            loop {
                let event = net_event_take::call();
                if event == EVENT_NONE {
                    return Poll::Pending;
                }
                match sort_event(event, awaiting) {
                    Sorted::Done(finish) => return Poll::Ready(finish),
                    Sorted::RememberLinkLost => *link_lost = true,
                    Sorted::Ignore => {}
                }
            }
        });
        match limit {
            None => Some(finished.await),
            Some(limit) => match select(finished, Timer::after(limit)).await {
                Either::First(finish) => Some(finish),
                Either::Second(()) => None,
            },
        }
    }

    /// Take every event already waiting: they belong to what came before.
    /// A `link lost` among them is still remembered.
    fn drain_events(&mut self) {
        loop {
            let event = net_event_take::call();
            if event == EVENT_NONE {
                return;
            }
            if sort_event(event, Awaiting::Nothing) == Sorted::RememberLinkLost {
                self.link_lost = true;
            }
        }
    }

    fn signal_of(&self, ssid: &str) -> Option<i8> {
        self.last_scan
            .iter()
            .find(|network| network.ssid == ssid)
            .map(|network| network.rssi)
    }
}

impl Default for SeamStation {
    fn default() -> Self {
        Self::new()
    }
}

impl StationControl for SeamStation {
    async fn scan(&mut self) -> Option<Vec<HeardNetwork>> {
        self.drain_events();
        let answered = net_scan_start::call() != 0
            && self
                .wait_for(Awaiting::Scan, Some(SCAN_LIMIT))
                .await
                .is_some();
        if !answered {
            log::warn!("[wifi] scan gave no answer");
            return None;
        }
        let mut records = vec![0u8; SCAN_MAX_NETWORKS * scan_record_len(MAX_SSID_LEN)];
        let count = net_scan_take::call(records.as_mut_ptr(), records.len() as u32);
        let heard = parse_scan(&records, count as usize);
        self.last_scan = heard.clone();
        Some(heard)
    }

    async fn connect(&mut self, ssid: &str, password: &str) -> ConnectOutcome {
        // What was queued belongs to the last attempt or the last link,
        // and so does a remembered loss.
        self.drain_events();
        self.link_lost = false;
        self.joined_rssi = None;
        // A name or a password longer than the seam carries is refused
        // here, never cut short.
        let started = ssid.len() <= MAX_SSID_LEN
            && password.len() <= MAX_PASSWORD_LEN
            && net_connect::call(
                ssid.as_ptr(),
                ssid.len() as u32,
                password.as_ptr(),
                password.len() as u32,
            ) != 0;
        if !started {
            log::warn!("[wifi] seam: the attempt was refused");
            return ConnectOutcome::Ended;
        }
        let outcome = match self.wait_for(Awaiting::Connect, Some(CONNECT_LIMIT)).await {
            Some(Finish::Connect(outcome)) => outcome,
            Some(_) => ConnectOutcome::Ended,
            None => ConnectOutcome::NotHeard,
        };
        match outcome {
            ConnectOutcome::Associated => {
                self.joined_rssi = self.signal_of(ssid);
                // The frame device reads its link on its next poll.
                NET_FRAMES.wake();
            }
            ConnectOutcome::AuthFailed => {
                log::info!("[wifi] attempt ended: the password was refused")
            }
            ConnectOutcome::NotHeard => log::info!("[wifi] attempt ended: not heard"),
            ConnectOutcome::Ended => log::info!("[wifi] attempt ended"),
        }
        outcome
    }

    async fn disconnect(&mut self) {
        self.joined_rssi = None;
        if net_link::call() == 0 {
            return;
        }
        net_disconnect::call();
        self.link_lost = false;
        // The link went down with no event (the seam's rule), so no wake
        // will tell the frame device: tell it here.
        NET_FRAMES.wake();
    }

    async fn wait_link_lost(&mut self) {
        let remembered = core::mem::take(&mut self.link_lost);
        if !remembered && net_link::call() != 0 {
            self.wait_for(Awaiting::LinkLost, None).await;
        }
        log::info!("[wifi] link lost");
        self.joined_rssi = None;
    }

    fn rssi(&self) -> Option<i8> {
        self.joined_rssi
    }
}

/// What the station is waiting for while it takes events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Awaiting {
    /// A scan's `scan done`.
    Scan,
    /// A connect's outcome.
    Connect,
    /// The joined network's `link lost`.
    LinkLost,
    /// Nothing: the events are stale.
    Nothing,
}

/// The event that ends a wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    ScanDone,
    Connect(ConnectOutcome),
    LinkLost,
}

/// What one event means while waiting for something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sorted {
    /// It ends the wait.
    Done(Finish),
    /// A `link lost` the wait was not for: keep it for `wait_link_lost`.
    RememberLinkLost,
    /// An event that means nothing now (a late answer to a wait that timed
    /// out), or a code `lp_seam::net` does not define.
    Ignore,
}

/// Sort one station event (`lp_seam::net::EVENT_*`, never `EVENT_NONE`)
/// against what the station is waiting for.
pub fn sort_event(event: u32, awaiting: Awaiting) -> Sorted {
    match (event, awaiting) {
        (EVENT_SCAN_DONE, Awaiting::Scan) => Sorted::Done(Finish::ScanDone),
        (EVENT_ASSOCIATED, Awaiting::Connect) => {
            Sorted::Done(Finish::Connect(ConnectOutcome::Associated))
        }
        (EVENT_AUTH_FAILED, Awaiting::Connect) => {
            Sorted::Done(Finish::Connect(ConnectOutcome::AuthFailed))
        }
        (EVENT_NOT_FOUND, Awaiting::Connect) => {
            Sorted::Done(Finish::Connect(ConnectOutcome::NotHeard))
        }
        (EVENT_LINK_LOST, Awaiting::LinkLost) => Sorted::Done(Finish::LinkLost),
        (EVENT_LINK_LOST, _) => Sorted::RememberLinkLost,
        _ => Sorted::Ignore,
    }
}

/// The networks in `count` scan records (`lp_seam::net`: name length u8,
/// the name, signal i8, secure u8). Stops at a record the bytes cut short;
/// skips a record with an empty name (hidden networks are omitted) or one
/// that is not UTF-8 (a name the station could never be told to join).
pub fn parse_scan(bytes: &[u8], count: usize) -> Vec<HeardNetwork> {
    let mut heard = Vec::new();
    let mut at = 0;
    for _ in 0..count {
        let Some(&name_len) = bytes.get(at) else {
            break;
        };
        let name_len = usize::from(name_len);
        let Some(record) = bytes.get(at..at + scan_record_len(name_len)) else {
            break;
        };
        at += record.len();
        let name = &record[1..1 + name_len];
        let (rssi, secure) = (record[1 + name_len] as i8, record[2 + name_len]);
        match core::str::from_utf8(name) {
            Ok(ssid) if !ssid.is_empty() => heard.push(HeardNetwork {
                ssid: String::from(ssid),
                rssi,
                secure: secure != 0,
            }),
            _ => {}
        }
    }
    heard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_wait_ends_on_its_own_events_only() {
        assert_eq!(
            sort_event(EVENT_SCAN_DONE, Awaiting::Scan),
            Sorted::Done(Finish::ScanDone)
        );
        assert_eq!(
            sort_event(EVENT_ASSOCIATED, Awaiting::Connect),
            Sorted::Done(Finish::Connect(ConnectOutcome::Associated))
        );
        assert_eq!(
            sort_event(EVENT_AUTH_FAILED, Awaiting::Connect),
            Sorted::Done(Finish::Connect(ConnectOutcome::AuthFailed))
        );
        assert_eq!(
            sort_event(EVENT_NOT_FOUND, Awaiting::Connect),
            Sorted::Done(Finish::Connect(ConnectOutcome::NotHeard))
        );
        assert_eq!(
            sort_event(EVENT_LINK_LOST, Awaiting::LinkLost),
            Sorted::Done(Finish::LinkLost)
        );
        // A late answer to another wait means nothing now.
        for awaiting in [Awaiting::Scan, Awaiting::LinkLost, Awaiting::Nothing] {
            for event in [EVENT_ASSOCIATED, EVENT_AUTH_FAILED, EVENT_NOT_FOUND] {
                assert_eq!(sort_event(event, awaiting), Sorted::Ignore);
            }
        }
        for awaiting in [Awaiting::Connect, Awaiting::LinkLost, Awaiting::Nothing] {
            assert_eq!(sort_event(EVENT_SCAN_DONE, awaiting), Sorted::Ignore);
        }
    }

    #[test]
    fn a_link_lost_during_another_wait_is_remembered() {
        for awaiting in [Awaiting::Scan, Awaiting::Connect, Awaiting::Nothing] {
            assert_eq!(
                sort_event(EVENT_LINK_LOST, awaiting),
                Sorted::RememberLinkLost
            );
        }
    }

    #[test]
    fn an_undefined_code_is_ignored() {
        assert_eq!(sort_event(6, Awaiting::Scan), Sorted::Ignore);
        assert_eq!(sort_event(u32::MAX, Awaiting::LinkLost), Sorted::Ignore);
    }

    #[test]
    fn scan_records_parse_in_order() {
        let mut bytes = Vec::new();
        record(&mut bytes, b"home", -42, 1);
        record(&mut bytes, b"cafe", -70, 0);
        let heard = parse_scan(&bytes, 2);
        assert_eq!(
            heard,
            [
                HeardNetwork {
                    ssid: "home".into(),
                    rssi: -42,
                    secure: true,
                },
                HeardNetwork {
                    ssid: "cafe".into(),
                    rssi: -70,
                    secure: false,
                },
            ]
        );
    }

    #[test]
    fn the_count_bounds_the_records_read() {
        let mut bytes = Vec::new();
        record(&mut bytes, b"a", -1, 0);
        record(&mut bytes, b"b", -2, 0);
        // The buffer's zeroed tail is not a third record.
        bytes.extend_from_slice(&[0; 16]);
        assert_eq!(parse_scan(&bytes, 1).len(), 1);
        assert_eq!(parse_scan(&bytes, 0).len(), 0);
    }

    #[test]
    fn hidden_and_non_utf8_names_are_skipped_and_a_cut_record_ends_the_list() {
        let mut bytes = Vec::new();
        record(&mut bytes, b"", -30, 1);
        record(&mut bytes, &[0xff, 0xfe], -31, 1);
        record(&mut bytes, b"kept", -32, 1);
        record(&mut bytes, b"cut short", -33, 1);
        bytes.truncate(bytes.len() - 1);
        let heard = parse_scan(&bytes, 4);
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].ssid, "kept");
        assert_eq!(heard[0].rssi, -32);
    }

    #[test]
    fn the_longest_names_fit_the_scan_buffer() {
        let mut bytes = Vec::new();
        for i in 0..SCAN_MAX_NETWORKS {
            let mut name = [b'n'; MAX_SSID_LEN];
            name[0] = b'a' + (i % 26) as u8;
            record(&mut bytes, &name, -50, 1);
        }
        assert_eq!(
            bytes.len(),
            SCAN_MAX_NETWORKS * scan_record_len(MAX_SSID_LEN)
        );
        assert_eq!(
            parse_scan(&bytes, SCAN_MAX_NETWORKS).len(),
            SCAN_MAX_NETWORKS
        );
    }

    #[test]
    fn a_station_that_never_scanned_has_no_signal() {
        let station = SeamStation::new();
        assert_eq!(station.rssi(), None);
        assert_eq!(station.signal_of("home"), None);
    }

    fn record(out: &mut Vec<u8>, name: &[u8], rssi: i8, secure: u8) {
        out.push(name.len() as u8);
        out.extend_from_slice(name);
        out.push(rssi as u8);
        out.push(secure);
    }
}
