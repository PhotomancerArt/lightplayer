//! What the station and the server share across threads.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lpc_access::NetworkFile;
use lpc_wire::{HeardNetwork, LastAttempt, NetworkScan, StationState};

use crate::net::scan_cache::ScanCache;
use crate::net::station_policy::StationPolicy;

/// The board between the station (on its own thread, `lp-net` on the C6)
/// and the server's probes (on the main thread): one `static` per image.
///
/// - The **server side** hands it the network file whenever it changes
///   ([`Self::settings_changed`], from the server's `NetworkChanged` hook and
///   once at boot), and reads the station's state, each network's last
///   attempt and the scan answer back ([`Self::station_state`],
///   [`Self::last_attempt`], [`Self::scan_answer`]). **Only the server's
///   thread ever touches the filesystem**: the station never reads the file
///   itself.
/// - The **station side** takes the file ([`Self::take_settings`]),
///   publishes the policy ([`Self::publish`]), records every scan
///   ([`Self::record_scan`]) and waits on [`Self::wait`] for either side's
///   news.
///
/// Every access is a short critical section (a clone at most); nothing
/// waits under it. The file holds the passwords: it lives here in RAM for
/// the station's connect and is never logged.
pub struct StationBoard {
    inner: Mutex<RefCell<Inner>>,
    /// The station has news: a settings change, or a client wants a scan.
    wake: Signal<CriticalSectionRawMutex, ()>,
    /// "Set to use Wi-Fi" (plan Q2), readable from any context without the
    /// lock: the radio driver's endpoint status reads it.
    uses_wifi: AtomicBool,
}

struct Inner {
    state: StationState,
    last: Vec<(String, LastAttempt)>,
    scan: ScanCache,
    settings: Option<NetworkFile>,
}

impl StationBoard {
    /// An empty board: `notConnected`, nothing heard, no settings yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(Inner {
                state: StationState::NotConnected,
                last: Vec::new(),
                scan: ScanCache::new(),
                settings: None,
            })),
            wake: Signal::new(),
            uses_wifi: AtomicBool::new(false),
        }
    }

    // --- the server's side ------------------------------------------------

    /// The network file as it now stands: the station reads it at its next
    /// wake. Also decides "set to use Wi-Fi" at once.
    pub fn settings_changed(&self, file: &NetworkFile) {
        self.uses_wifi
            .store(file.wifi && !file.networks.is_empty(), Ordering::Release);
        critical_section::with(|cs| {
            self.inner.borrow_ref_mut(cs).settings = Some(file.clone());
        });
        self.wake.signal(());
    }

    /// The board is set to use Wi-Fi: the switch on and a network saved.
    #[must_use]
    pub fn uses_wifi(&self) -> bool {
        self.uses_wifi.load(Ordering::Acquire)
    }

    /// What the station is doing.
    #[must_use]
    pub fn station_state(&self) -> StationState {
        critical_section::with(|cs| self.inner.borrow_ref(cs).state.clone())
    }

    /// How the station's last attempt at `ssid` went.
    #[must_use]
    pub fn last_attempt(&self, ssid: &str) -> Option<LastAttempt> {
        critical_section::with(|cs| {
            self.inner
                .borrow_ref(cs)
                .last
                .iter()
                .find(|(name, _)| name == ssid)
                .map(|(_, last)| *last)
        })
    }

    /// The answer to a client's scan at `now_ms`: a fresh list, or
    /// `scanning` (and the station is woken to listen).
    pub fn scan_answer(&self, now_ms: u64) -> NetworkScan {
        let answer = critical_section::with(|cs| self.inner.borrow_ref_mut(cs).scan.answer(now_ms));
        if answer == NetworkScan::Scanning {
            self.wake.signal(());
        }
        answer
    }

    // --- the station's side -----------------------------------------------

    /// The network file handed over since the last take.
    pub fn take_settings(&self) -> Option<NetworkFile> {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).settings.take())
    }

    /// Publish the policy's state and its per-network attempts.
    pub fn publish(&self, policy: &StationPolicy) {
        let state = policy.state().clone();
        let last = policy.attempts().to_vec();
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow_ref_mut(cs);
            inner.state = state;
            inner.last = last;
        });
    }

    /// A scan finished at `now_ms`.
    pub fn record_scan(&self, now_ms: u64, heard: Vec<HeardNetwork>) {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).scan.record(now_ms, heard));
    }

    /// Whether a client asked for a scan since the last one; clears it.
    pub fn take_scan_wanted(&self) -> bool {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).scan.take_wanted())
    }

    /// Wait for news from the server's side.
    pub async fn wait(&self) {
        self.wake.wait().await;
    }
}

impl Default for StationBoard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::station_policy::{StationAction, StationEvent};
    use crate::net::station_settings::StationSettings;
    use lpc_access::WifiNetwork;

    fn file(networks: &[&str]) -> NetworkFile {
        let mut file = NetworkFile::none();
        for ssid in networks {
            file.add(WifiNetwork {
                ssid: String::from(*ssid),
                password: String::from("correct-horse-42"),
                hidden: false,
            })
            .unwrap();
        }
        file
    }

    #[test]
    fn a_settings_change_reaches_the_station_once_and_decides_uses_wifi() {
        let board = StationBoard::new();
        assert!(!board.uses_wifi());
        board.settings_changed(&file(&["lp-walk-net"]));
        assert!(board.uses_wifi());
        assert_eq!(board.take_settings().unwrap().networks.len(), 1);
        assert!(board.take_settings().is_none());
        board.settings_changed(&file(&[]));
        assert!(!board.uses_wifi(), "nothing saved is not using Wi-Fi");
    }

    #[test]
    fn the_policy_is_what_the_probes_read() {
        let board = StationBoard::new();
        let mut policy = StationPolicy::new(String::from("lp-8e30.local"));
        let settings = StationSettings::from_file(&file(&["lp-walk-net"]));
        assert_eq!(
            policy.handle(0, StationEvent::SettingsChanged(settings)),
            [StationAction::Scan]
        );
        policy.handle(
            10,
            StationEvent::ScanDone(alloc::vec![HeardNetwork {
                ssid: String::from("lp-walk-net"),
                rssi: -50,
                secure: true,
            }]),
        );
        policy.handle(20, StationEvent::AuthFailed);
        board.publish(&policy);
        assert!(matches!(board.station_state(), StationState::Failed { .. }));
        assert_eq!(
            board.last_attempt("lp-walk-net"),
            Some(LastAttempt::WrongPassword)
        );
    }

    #[test]
    fn a_stale_scan_wants_a_new_one() {
        let board = StationBoard::new();
        assert_eq!(board.scan_answer(0), NetworkScan::Scanning);
        assert!(board.take_scan_wanted());
        board.record_scan(100, alloc::vec![]);
        assert_eq!(board.scan_answer(200), NetworkScan::Heard(alloc::vec![]));
    }
}
