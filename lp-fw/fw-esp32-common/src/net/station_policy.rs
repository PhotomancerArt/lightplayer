//! The station's join policy: a sans-IO state machine.
//!
//! [`StationPolicy`] is fed what the radio and the settings say
//! ([`StationEvent`]) with the time (`now_ms`, monotonic milliseconds, the
//! caller's clock) and answers what the radio should do next
//! ([`StationAction`]). It keeps the wire's [`StationState`] and each saved
//! network's [`LastAttempt`], both in RAM only. It never touches a radio, a
//! clock or a file: the chip's station task does (`fw-esp32c6`'s
//! `net/station_task.rs`), and the network seam's emulator answer (Wi-Fi
//! roadmap M6, PR C) drives it the same way.
//!
//! The rules, each pinned by a test below:
//!
//! - **The Wi-Fi switch off**: [`StationState::Off`], no scans, and any
//!   network the station is on is left.
//! - **Nothing saved**: [`StationState::NotConnected`] and **no scan, ever**
//!   — the fyeah sign's board keeps ESP-NOW on its channel (plan Q2).
//! - **The join rule** ([`choose`]): the strongest saved network heard,
//!   skipping one whose password was refused until that password changes;
//!   hidden saved networks by name after the heard ones.
//! - **A just-added network, or one whose password changed, is tried at
//!   once** (plan Q8), even while the board is on another. If it fails the
//!   policy goes back to the strongest other saved network.
//! - One attempt reports `Connecting { step }` as it advances: `looking`
//!   (scanning for it), `checkingPassword` (associating), `gettingAddress`
//!   (joined, waiting on DHCP). A refused password ends it
//!   [`StationFailure::WrongPassword`]; not heard, or not associated within
//!   [`JOIN_TIMEOUT_MS`], [`StationFailure::NotFound`]; joined with no
//!   address in [`ADDRESS_TIMEOUT_MS`], [`StationFailure::NoAddress`].
//! - **Searching backs off** ([`StationBackoff`]): at once, every 10 s for a
//!   minute, then every minute; a settings change or a lost link starts it
//!   over.
//! - A failure stays the reported state until the next attempt begins, so a
//!   client watching for its network's result sees it.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_wire::{ConnectStep, HeardNetwork, LastAttempt, StationFailure, StationState};

use crate::net::join_choice::{JoinChoice, choose};
use crate::net::station_backoff::StationBackoff;
use crate::net::station_settings::StationSettings;

/// How long a scan may take before the policy treats it as having heard
/// nothing (a real one takes about two seconds).
pub const SCAN_TIMEOUT_MS: u64 = 10_000;
/// How long an attempt may take to associate (the password check included).
pub const JOIN_TIMEOUT_MS: u64 = 15_000;
/// How long a joined station may wait for an address.
pub const ADDRESS_TIMEOUT_MS: u64 = 10_000;

/// What the radio and the settings tell the policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StationEvent {
    /// The network file was read (at start, and after every change).
    SettingsChanged(StationSettings),
    /// A scan the policy asked for finished: what was heard, any order.
    ScanDone(Vec<HeardNetwork>),
    /// The attempt associated: the password was accepted.
    Associated,
    /// The network refused the password.
    AuthFailed,
    /// The attempt ended because the station did not hear the network.
    NotHeard,
    /// DHCP gave the station this IPv4 address.
    AddressAcquired([u8; 4]),
    /// The station lost the network (or an attempt ended with no reason).
    LinkLost,
    /// The connected network's signal, in dBm.
    Signal(i8),
    /// Time passed; deadlines are checked against `now_ms`.
    Tick,
}

/// What the policy asks the radio to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StationAction {
    /// Listen for networks; answer with [`StationEvent::ScanDone`].
    Scan,
    /// Join the saved network `ssid` with its saved password (the station
    /// task looks the password up; the policy never holds it).
    Connect { ssid: String },
    /// Leave whatever network the station is on or trying.
    Disconnect,
}

/// The join policy. See the module doc for its rules.
#[derive(Debug, Clone)]
pub struct StationPolicy {
    host: String,
    settings: Option<StationSettings>,
    phase: Phase,
    state: StationState,
    backoff: StationBackoff,
    last: Vec<(String, LastAttempt)>,
    /// Networks whose password was refused, with the refused password's tag.
    refused: Vec<(String, u32)>,
    /// Networks tried since the last scan.
    tried: Vec<String>,
    /// The last scan's list.
    heard: Vec<HeardNetwork>,
    /// A just-added (or re-keyed) network to try before anything else.
    try_now: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    /// Off, nothing saved, or no settings yet: the radio is left alone.
    Idle,
    /// A scan is out, until `until`.
    Scanning { until: u64 },
    /// Nothing to join was heard; look again at `until`.
    Waiting { until: u64 },
    /// One attempt at `ssid`, at `step`, until `until`.
    Joining {
        ssid: String,
        rssi: i8,
        step: ConnectStep,
        until: u64,
    },
    /// On `ssid`.
    Connected { ssid: String },
}

impl StationPolicy {
    /// A policy for a board whose LAN name is `host` (`lp-xxxx.local`,
    /// reported in [`StationState::Connected`]). It does nothing until the
    /// first [`StationEvent::SettingsChanged`].
    #[must_use]
    pub fn new(host: String) -> Self {
        Self {
            host,
            settings: None,
            phase: Phase::Idle,
            state: StationState::NotConnected,
            backoff: StationBackoff::new(),
            last: Vec::new(),
            refused: Vec::new(),
            tried: Vec::new(),
            heard: Vec::new(),
            try_now: None,
        }
    }

    /// Take one event at `now_ms`; the actions to run, in order.
    pub fn handle(&mut self, now_ms: u64, event: StationEvent) -> Vec<StationAction> {
        let mut actions = Vec::new();
        match event {
            StationEvent::SettingsChanged(settings) => {
                self.settings_changed(now_ms, settings, &mut actions);
            }
            StationEvent::ScanDone(heard) => self.scan_done(now_ms, heard, &mut actions),
            StationEvent::Associated => self.associated(now_ms),
            StationEvent::AuthFailed => {
                if let Some(ssid) = self.joining_ssid() {
                    let tag = self.tag_of(&ssid);
                    self.refused.retain(|(name, _)| *name != ssid);
                    self.refused.push((ssid.clone(), tag));
                    self.fail(now_ms, ssid, StationFailure::WrongPassword, &mut actions);
                }
            }
            StationEvent::NotHeard => {
                if let Some(ssid) = self.joining_ssid() {
                    self.fail(now_ms, ssid, StationFailure::NotFound, &mut actions);
                }
            }
            StationEvent::AddressAcquired(ip) => self.address(now_ms, ip),
            StationEvent::LinkLost => self.link_lost(now_ms, &mut actions),
            StationEvent::Signal(rssi) => {
                if let StationState::Connected { rssi: now, .. } = &mut self.state {
                    *now = rssi;
                }
            }
            StationEvent::Tick => self.tick(now_ms, &mut actions),
        }
        actions
    }

    /// What the station is doing, as the wire reports it.
    #[must_use]
    pub fn state(&self) -> &StationState {
        &self.state
    }

    /// How the last attempt at the saved network `ssid` went, since start.
    #[must_use]
    pub fn last_attempt(&self, ssid: &str) -> Option<LastAttempt> {
        self.last
            .iter()
            .find(|(name, _)| name == ssid)
            .map(|(_, last)| *last)
    }

    /// Every saved network's last attempt since start, by name.
    #[must_use]
    pub fn attempts(&self) -> &[(String, LastAttempt)] {
        &self.last
    }

    /// When the policy next needs a [`StationEvent::Tick`]; `None` while it
    /// only waits on the radio or the settings.
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        match &self.phase {
            Phase::Scanning { until } | Phase::Waiting { until } | Phase::Joining { until, .. } => {
                Some(*until)
            }
            Phase::Idle | Phase::Connected { .. } => None,
        }
    }

    /// The board is set to use Wi-Fi (switch on, a network saved). See
    /// [`StationSettings::uses_wifi`].
    #[must_use]
    pub fn uses_wifi(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(StationSettings::uses_wifi)
    }

    fn settings_changed(
        &mut self,
        now: u64,
        settings: StationSettings,
        actions: &mut Vec<StationAction>,
    ) {
        let old = self.settings.replace(settings.clone());
        // Memory of networks no longer saved goes; a refusal goes when the
        // password it refused changed.
        self.last
            .retain(|(ssid, _)| settings.network(ssid).is_some());
        self.refused.retain(|(ssid, tag)| {
            settings
                .network(ssid)
                .is_some_and(|saved| saved.secret_tag == *tag)
        });
        if !settings.wifi || settings.networks.is_empty() {
            self.leave(actions);
            self.phase = Phase::Idle;
            self.try_now = None;
            self.state = if settings.wifi {
                StationState::NotConnected
            } else {
                StationState::Off
            };
            return;
        }
        // The newest network that was not saved before, or whose password
        // changed: tried at once. None at start (nothing was known before).
        let fresh = old.as_ref().and_then(|old| {
            settings
                .networks
                .iter()
                .rev()
                .find(|saved| {
                    old.network(&saved.ssid)
                        .is_none_or(|before| before.secret_tag != saved.secret_tag)
                })
                .map(|saved| saved.ssid.clone())
        });
        self.backoff.restart(now);
        self.tried.clear();
        if fresh.is_none() {
            let still_saved = |ssid: &str| settings.network(ssid).is_some();
            match &self.phase {
                Phase::Connected { ssid } | Phase::Joining { ssid, .. } if still_saved(ssid) => {
                    return;
                }
                Phase::Scanning { .. } => return,
                _ => {}
            }
        }
        self.leave(actions);
        if let Some(ssid) = fresh {
            self.try_now = Some(ssid);
            self.begin_try_now(now, actions);
        } else {
            self.start_search(now, actions);
        }
    }

    fn begin_try_now(&mut self, now: u64, actions: &mut Vec<StationAction>) {
        let Some(ssid) = self.try_now.clone() else {
            return;
        };
        if self.settings_network_hidden(&ssid) {
            self.start_attempt(now, JoinChoice::Hidden { ssid }, actions);
            return;
        }
        self.state = StationState::Connecting {
            ssid,
            step: ConnectStep::Looking,
        };
        self.phase = Phase::Scanning {
            until: now + SCAN_TIMEOUT_MS,
        };
        actions.push(StationAction::Scan);
    }

    fn start_search(&mut self, now: u64, actions: &mut Vec<StationAction>) {
        if !matches!(self.state, StationState::Failed { .. }) {
            self.state = StationState::NotConnected;
        }
        self.phase = Phase::Scanning {
            until: now + SCAN_TIMEOUT_MS,
        };
        actions.push(StationAction::Scan);
    }

    fn scan_done(&mut self, now: u64, heard: Vec<HeardNetwork>, actions: &mut Vec<StationAction>) {
        self.heard = heard;
        if !matches!(self.phase, Phase::Scanning { .. }) {
            return;
        }
        if let Some(target) = self.try_now.clone() {
            let rssi = self
                .heard
                .iter()
                .find(|network| network.ssid == target)
                .map(|network| network.rssi);
            match rssi {
                Some(rssi) => {
                    self.start_attempt(now, JoinChoice::Heard { ssid: target, rssi }, actions);
                }
                None => {
                    self.tried.push(target.clone());
                    self.fail(now, target, StationFailure::NotFound, actions);
                }
            }
            return;
        }
        self.choose_next(now, actions);
    }

    fn choose_next(&mut self, now: u64, actions: &mut Vec<StationAction>) {
        let choice = match &self.settings {
            Some(settings) => choose(settings, &self.heard, |ssid| {
                self.tried.iter().any(|tried| tried == ssid)
                    || self.refused.iter().any(|(name, _)| name == ssid)
            }),
            None => None,
        };
        match choice {
            Some(choice) => self.start_attempt(now, choice, actions),
            None => {
                self.tried.clear();
                if matches!(self.state, StationState::Connecting { .. }) {
                    self.state = StationState::NotConnected;
                }
                self.phase = Phase::Waiting {
                    until: self.backoff.next_after(now),
                };
            }
        }
    }

    fn start_attempt(&mut self, now: u64, choice: JoinChoice, actions: &mut Vec<StationAction>) {
        let (ssid, rssi, step) = match choice {
            JoinChoice::Heard { ssid, rssi } => (ssid, rssi, ConnectStep::CheckingPassword),
            JoinChoice::Hidden { ssid } => (ssid, 0, ConnectStep::Looking),
        };
        self.tried.push(ssid.clone());
        self.state = StationState::Connecting {
            ssid: ssid.clone(),
            step,
        };
        self.phase = Phase::Joining {
            ssid: ssid.clone(),
            rssi,
            step,
            until: now + JOIN_TIMEOUT_MS,
        };
        actions.push(StationAction::Connect { ssid });
    }

    fn associated(&mut self, now: u64) {
        if let Phase::Joining {
            ssid, step, until, ..
        } = &mut self.phase
        {
            if *step != ConnectStep::GettingAddress {
                *step = ConnectStep::GettingAddress;
                *until = now + ADDRESS_TIMEOUT_MS;
                self.state = StationState::Connecting {
                    ssid: ssid.clone(),
                    step: ConnectStep::GettingAddress,
                };
            }
        }
    }

    fn address(&mut self, now: u64, ip: [u8; 4]) {
        let Phase::Joining { ssid, rssi, .. } = &self.phase else {
            return;
        };
        let (ssid, rssi) = (ssid.clone(), *rssi);
        self.set_last(&ssid, LastAttempt::Connected);
        self.state = StationState::Connected {
            ssid: ssid.clone(),
            ip: alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]),
            rssi,
            host: self.host.clone(),
        };
        self.phase = Phase::Connected { ssid };
        self.tried.clear();
        self.try_now = None;
        self.backoff.restart(now);
    }

    fn link_lost(&mut self, now: u64, actions: &mut Vec<StationAction>) {
        match &self.phase {
            Phase::Connected { .. } => {
                self.state = StationState::NotConnected;
                self.backoff.restart(now);
                self.tried.clear();
                self.start_search(now, actions);
            }
            Phase::Joining { ssid, step, .. } => {
                let reason = if *step == ConnectStep::GettingAddress {
                    StationFailure::NoAddress
                } else {
                    StationFailure::NotFound
                };
                let ssid = ssid.clone();
                self.fail(now, ssid, reason, actions);
            }
            Phase::Idle | Phase::Scanning { .. } | Phase::Waiting { .. } => {}
        }
    }

    fn tick(&mut self, now: u64, actions: &mut Vec<StationAction>) {
        match &self.phase {
            Phase::Scanning { until } if now >= *until => {
                self.scan_done(now, Vec::new(), actions);
            }
            Phase::Waiting { until } if now >= *until => self.start_search(now, actions),
            Phase::Joining {
                ssid, step, until, ..
            } if now >= *until => {
                let reason = if *step == ConnectStep::GettingAddress {
                    StationFailure::NoAddress
                } else {
                    StationFailure::NotFound
                };
                let ssid = ssid.clone();
                self.fail(now, ssid, reason, actions);
            }
            _ => {}
        }
    }

    /// The attempt at `ssid` ended with `reason`: record it, leave, and go
    /// on to the strongest other saved network (from the last scan).
    fn fail(
        &mut self,
        now: u64,
        ssid: String,
        reason: StationFailure,
        actions: &mut Vec<StationAction>,
    ) {
        self.set_last(&ssid, last_of(reason));
        if self.try_now.as_deref() == Some(ssid.as_str()) {
            self.try_now = None;
        }
        if matches!(self.phase, Phase::Joining { .. }) {
            actions.push(StationAction::Disconnect);
        }
        self.state = StationState::Failed { ssid, reason };
        self.choose_next(now, actions);
    }

    /// Leave whatever network the station is on or trying.
    fn leave(&mut self, actions: &mut Vec<StationAction>) {
        if matches!(self.phase, Phase::Joining { .. } | Phase::Connected { .. }) {
            actions.push(StationAction::Disconnect);
        }
    }

    fn joining_ssid(&self) -> Option<String> {
        match &self.phase {
            Phase::Joining { ssid, .. } => Some(ssid.clone()),
            _ => None,
        }
    }

    fn tag_of(&self, ssid: &str) -> u32 {
        self.settings
            .as_ref()
            .and_then(|settings| settings.network(ssid))
            .map_or(0, |saved| saved.secret_tag)
    }

    fn settings_network_hidden(&self, ssid: &str) -> bool {
        self.settings
            .as_ref()
            .and_then(|settings| settings.network(ssid))
            .is_some_and(|saved| saved.hidden)
    }

    fn set_last(&mut self, ssid: &str, last: LastAttempt) {
        match self.last.iter_mut().find(|(name, _)| name == ssid) {
            Some((_, slot)) => *slot = last,
            None => self.last.push((String::from(ssid), last)),
        }
    }
}

fn last_of(reason: StationFailure) -> LastAttempt {
    match reason {
        StationFailure::WrongPassword => LastAttempt::WrongPassword,
        StationFailure::NotFound => LastAttempt::NotFound,
        StationFailure::NoAddress => LastAttempt::NoAddress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::station_settings::{SavedNetwork, secret_tag};
    use alloc::vec;

    const HOME: &str = "lp-walk-net";
    const OFFICE: &str = "lp-back-office";
    const ATTIC: &str = "lp-attic";
    const HOST: &str = "lp-8e30.local";

    #[test]
    fn the_wifi_switch_off_is_off_and_never_scans() {
        let mut rig = Rig::new();
        assert_eq!(rig.settings(false, &[(HOME, "pw-one-long")]), vec![]);
        assert_eq!(rig.state(), StationState::Off);
        assert_eq!(rig.after(600_000), vec![]);
        assert_eq!(rig.policy.next_wake(), None);
    }

    #[test]
    fn nothing_saved_never_scans() {
        let mut rig = Rig::new();
        assert_eq!(rig.settings(true, &[]), vec![]);
        assert_eq!(rig.state(), StationState::NotConnected);
        assert_eq!(rig.after(3_600_000), vec![]);
        assert!(!rig.policy.uses_wifi());
    }

    #[test]
    fn a_saved_network_heard_is_joined_through_every_step() {
        let mut rig = Rig::new();
        assert_eq!(
            rig.settings(true, &[(HOME, "pw-one-long")]),
            vec![StationAction::Scan]
        );
        assert_eq!(rig.state(), StationState::NotConnected);
        assert_eq!(rig.heard(&[(HOME, -55)]), vec![connect(HOME)]);
        assert_eq!(rig.state(), connecting(HOME, ConnectStep::CheckingPassword));
        assert_eq!(rig.event(StationEvent::Associated), vec![]);
        assert_eq!(rig.state(), connecting(HOME, ConnectStep::GettingAddress));
        assert_eq!(
            rig.event(StationEvent::AddressAcquired([192, 168, 1, 40])),
            vec![]
        );
        assert_eq!(
            rig.state(),
            StationState::Connected {
                ssid: HOME.into(),
                ip: "192.168.1.40".into(),
                rssi: -55,
                host: HOST.into()
            }
        );
        assert_eq!(rig.policy.last_attempt(HOME), Some(LastAttempt::Connected));
        assert!(rig.policy.uses_wifi());
        rig.event(StationEvent::Signal(-61));
        assert!(matches!(
            rig.state(),
            StationState::Connected { rssi: -61, .. }
        ));
    }

    #[test]
    fn the_strongest_saved_network_heard_is_tried_first() {
        let mut rig = Rig::new();
        rig.settings(true, &[(OFFICE, "pw-two-long"), (HOME, "pw-one-long")]);
        assert_eq!(
            rig.heard(&[(OFFICE, -40), (HOME, -70)]),
            vec![connect(OFFICE)]
        );
    }

    #[test]
    fn a_refused_password_is_wrong_password_and_skipped_until_it_changes() {
        let mut rig = Rig::new();
        rig.settings(true, &[(HOME, "pw-one-long"), (OFFICE, "pw-two-long")]);
        rig.heard(&[(HOME, -40), (OFFICE, -70)]);
        assert_eq!(
            rig.event(StationEvent::AuthFailed),
            vec![StationAction::Disconnect, connect(OFFICE)]
        );
        assert_eq!(
            rig.policy.last_attempt(HOME),
            Some(LastAttempt::WrongPassword)
        );
        // OFFICE fails too; the next search skips HOME though it is loudest.
        rig.event(StationEvent::NotHeard);
        assert_eq!(rig.state(), failed(OFFICE, StationFailure::NotFound));
        let wake = rig.policy.next_wake().unwrap();
        assert_eq!(rig.after(wake - rig.now), vec![StationAction::Scan]);
        assert_eq!(
            rig.heard(&[(HOME, -40), (OFFICE, -70)]),
            vec![connect(OFFICE)]
        );
        // A new password for HOME lifts the refusal, and is tried at once.
        rig.event(StationEvent::NotHeard);
        assert_eq!(
            rig.settings(true, &[(HOME, "pw-one-fixed"), (OFFICE, "pw-two-long")]),
            vec![StationAction::Scan]
        );
        assert_eq!(rig.state(), connecting(HOME, ConnectStep::Looking));
        assert_eq!(rig.heard(&[(HOME, -40)]), vec![connect(HOME)]);
    }

    #[test]
    fn a_just_added_network_is_tried_at_once_even_while_connected() {
        let mut rig = Rig::connected_to_home();
        assert_eq!(
            rig.settings(true, &[(HOME, "pw-one-long"), (OFFICE, "pw-two-long")]),
            vec![StationAction::Disconnect, StationAction::Scan]
        );
        assert_eq!(rig.state(), connecting(OFFICE, ConnectStep::Looking));
        assert_eq!(
            rig.heard(&[(HOME, -40), (OFFICE, -70)]),
            vec![connect(OFFICE)]
        );
        assert_eq!(
            rig.state(),
            connecting(OFFICE, ConnectStep::CheckingPassword)
        );
    }

    #[test]
    fn a_just_added_network_that_fails_gives_way_to_the_strongest_other() {
        let mut rig = Rig::connected_to_home();
        rig.settings(true, &[(HOME, "pw-one-long"), (OFFICE, "pw-two-long")]);
        rig.heard(&[(HOME, -40), (OFFICE, -70)]);
        assert_eq!(
            rig.event(StationEvent::AuthFailed),
            vec![StationAction::Disconnect, connect(HOME)]
        );
        assert_eq!(
            rig.policy.last_attempt(OFFICE),
            Some(LastAttempt::WrongPassword)
        );
        // The failure is reported until the next attempt begins.
        assert_eq!(rig.state(), connecting(HOME, ConnectStep::CheckingPassword));
    }

    #[test]
    fn a_just_added_network_not_heard_is_not_found() {
        let mut rig = Rig::connected_to_home();
        rig.settings(true, &[(HOME, "pw-one-long"), (OFFICE, "pw-two-long")]);
        assert_eq!(rig.heard(&[(HOME, -40)]), vec![connect(HOME)]);
        assert_eq!(rig.policy.last_attempt(OFFICE), Some(LastAttempt::NotFound));
    }

    #[test]
    fn a_made_up_name_alone_ends_not_found_and_waits() {
        let mut rig = Rig::new();
        rig.settings(true, &[]);
        assert_eq!(
            rig.settings(true, &[("lp-made-up", "pw-made-up")]),
            vec![StationAction::Scan]
        );
        assert_eq!(rig.heard(&[(HOME, -40)]), vec![]);
        assert_eq!(rig.state(), failed("lp-made-up", StationFailure::NotFound));
        assert_eq!(
            rig.policy.next_wake(),
            Some(rig.now + StationBackoff::QUICK_RETRIES_MS[0])
        );
    }

    #[test]
    fn joined_with_no_address_in_ten_seconds_is_no_address() {
        let mut rig = Rig::new();
        rig.settings(true, &[(HOME, "pw-one-long")]);
        rig.heard(&[(HOME, -40)]);
        rig.event(StationEvent::Associated);
        assert_eq!(rig.after(ADDRESS_TIMEOUT_MS - 1), vec![]);
        assert_eq!(rig.after(1), vec![StationAction::Disconnect]);
        assert_eq!(rig.state(), failed(HOME, StationFailure::NoAddress));
        assert_eq!(rig.policy.last_attempt(HOME), Some(LastAttempt::NoAddress));
    }

    #[test]
    fn not_associated_in_time_is_not_found() {
        let mut rig = Rig::new();
        rig.settings(true, &[(HOME, "pw-one-long")]);
        rig.heard(&[(HOME, -40)]);
        assert_eq!(rig.after(JOIN_TIMEOUT_MS), vec![StationAction::Disconnect]);
        assert_eq!(rig.state(), failed(HOME, StationFailure::NotFound));
    }

    #[test]
    fn a_lost_link_searches_again_at_once_and_backs_off_afresh() {
        let mut rig = Rig::connected_to_home();
        assert_eq!(rig.event(StationEvent::LinkLost), vec![StationAction::Scan]);
        assert_eq!(rig.state(), StationState::NotConnected);
        assert_eq!(rig.heard(&[]), vec![]);
        assert_eq!(rig.policy.next_wake(), Some(rig.now + 1_000));
    }

    /// A first scan that misses the access point is retried after a second
    /// and two more, then every ten seconds for the search's first minute,
    /// then every minute (FC6 re-check: 4 boots of 19 waited 10 s or more).
    #[test]
    fn searching_retries_quickly_then_backs_off_from_ten_seconds_to_a_minute() {
        let mut rig = Rig::new();
        rig.settings(true, &[(HOME, "pw-one-long")]);
        let mut gaps = Vec::new();
        for _ in 0..10 {
            rig.heard(&[]);
            let wake = rig.policy.next_wake().unwrap();
            gaps.push(wake - rig.now);
            assert_eq!(rig.after(wake - rig.now), vec![StationAction::Scan]);
        }
        assert_eq!(
            gaps,
            [
                1_000, 2_000, 10_000, 10_000, 10_000, 10_000, 10_000, 10_000, 60_000, 60_000
            ]
        );
    }

    #[test]
    fn a_scan_that_never_answers_counts_as_hearing_nothing() {
        let mut rig = Rig::new();
        rig.settings(true, &[(HOME, "pw-one-long")]);
        assert_eq!(rig.after(SCAN_TIMEOUT_MS), vec![]);
        assert!(matches!(rig.policy.next_wake(), Some(at) if at > rig.now));
    }

    #[test]
    fn hidden_networks_are_tried_by_name_after_the_heard_ones() {
        let mut rig = Rig::new();
        rig.policy.handle(
            0,
            StationEvent::SettingsChanged(settings_of(true, &[(HOME, "pw-one-long")], &[ATTIC])),
        );
        assert_eq!(rig.heard(&[(HOME, -40)]), vec![connect(HOME)]);
        assert_eq!(
            rig.event(StationEvent::AuthFailed),
            vec![StationAction::Disconnect, connect(ATTIC)]
        );
        assert_eq!(rig.state(), connecting(ATTIC, ConnectStep::Looking));
    }

    #[test]
    fn turning_wifi_off_leaves_the_network_and_forgetting_it_searches() {
        let mut rig = Rig::connected_to_home();
        assert_eq!(
            rig.settings(false, &[(HOME, "pw-one-long")]),
            vec![StationAction::Disconnect]
        );
        assert_eq!(rig.state(), StationState::Off);
        let mut rig = Rig::connected_to_home();
        assert_eq!(
            rig.settings(true, &[(OFFICE, "pw-two-long")]),
            vec![StationAction::Disconnect, StationAction::Scan]
        );
        assert_eq!(
            rig.policy.last_attempt(HOME),
            None,
            "forgotten means forgotten"
        );
    }

    #[test]
    fn an_unrelated_change_while_connected_keeps_the_network() {
        let mut rig = Rig::connected_to_home();
        rig.policy.handle(
            rig.now,
            StationEvent::SettingsChanged(settings_of(true, &[(HOME, "pw-one-long")], &[])),
        );
        assert!(matches!(rig.state(), StationState::Connected { .. }));
    }

    #[test]
    fn a_changed_password_for_the_current_network_rejoins_it_at_once() {
        let mut rig = Rig::connected_to_home();
        assert_eq!(
            rig.settings(true, &[(HOME, "pw-one-new")]),
            vec![StationAction::Disconnect, StationAction::Scan]
        );
        assert_eq!(rig.state(), connecting(HOME, ConnectStep::Looking));
    }

    #[test]
    fn events_out_of_turn_change_nothing() {
        let mut rig = Rig::new();
        rig.settings(true, &[]);
        for event in [
            StationEvent::Associated,
            StationEvent::AuthFailed,
            StationEvent::NotHeard,
            StationEvent::AddressAcquired([10, 0, 0, 7]),
            StationEvent::LinkLost,
            StationEvent::ScanDone(Vec::new()),
        ] {
            assert_eq!(rig.event(event), vec![]);
            assert_eq!(rig.state(), StationState::NotConnected);
        }
    }

    struct Rig {
        policy: StationPolicy,
        now: u64,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                policy: StationPolicy::new(HOST.into()),
                now: 1_000,
            }
        }

        fn connected_to_home() -> Self {
            let mut rig = Self::new();
            rig.settings(true, &[(HOME, "pw-one-long")]);
            rig.heard(&[(HOME, -40)]);
            rig.event(StationEvent::Associated);
            rig.event(StationEvent::AddressAcquired([192, 168, 1, 40]));
            assert!(matches!(rig.state(), StationState::Connected { .. }));
            rig
        }

        fn settings(&mut self, wifi: bool, networks: &[(&str, &str)]) -> Vec<StationAction> {
            self.event(StationEvent::SettingsChanged(settings_of(
                wifi,
                networks,
                &[],
            )))
        }

        fn heard(&mut self, list: &[(&str, i8)]) -> Vec<StationAction> {
            let heard = list
                .iter()
                .map(|(ssid, rssi)| HeardNetwork {
                    ssid: (*ssid).into(),
                    rssi: *rssi,
                    secure: true,
                })
                .collect();
            self.event(StationEvent::ScanDone(heard))
        }

        fn event(&mut self, event: StationEvent) -> Vec<StationAction> {
            self.now += 100;
            self.policy.handle(self.now, event)
        }

        fn after(&mut self, ms: u64) -> Vec<StationAction> {
            self.now += ms;
            self.policy.handle(self.now, StationEvent::Tick)
        }

        fn state(&self) -> StationState {
            self.policy.state().clone()
        }
    }

    fn settings_of(wifi: bool, networks: &[(&str, &str)], hidden: &[&str]) -> StationSettings {
        let mut list: Vec<SavedNetwork> = networks
            .iter()
            .map(|(ssid, password)| SavedNetwork {
                ssid: (*ssid).into(),
                hidden: false,
                secret_tag: secret_tag(password),
            })
            .collect();
        list.extend(hidden.iter().map(|ssid| SavedNetwork {
            ssid: (*ssid).into(),
            hidden: true,
            secret_tag: secret_tag("pw-hidden-one"),
        }));
        StationSettings {
            wifi,
            networks: list,
        }
    }

    fn connect(ssid: &str) -> StationAction {
        StationAction::Connect { ssid: ssid.into() }
    }

    fn connecting(ssid: &str, step: ConnectStep) -> StationState {
        StationState::Connecting {
            ssid: ssid.into(),
            step,
        }
    }

    fn failed(ssid: &str, reason: StationFailure) -> StationState {
        StationState::Failed {
            ssid: ssid.into(),
            reason,
        }
    }
}
