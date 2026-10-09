//! What the relay task and the server share across threads: the twin of
//! `net::StationBoard` for the cloud relay.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::RefCell;

use critical_section::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lp_link::Micros;
use lpc_access::{DeviceAccessFile, NetworkFile};
use lpc_relay::{RefuseReason, RelayAccount, RelayProjectFacts, RelayState};
use lpc_wire::{RelayRefusal, RelayState as WireRelayState};

use super::relay_driver::{RelayCounters, RelayDriver};
use super::relay_picture_mode::RelayPictureMode;
use super::relay_picture_slot::RelayPictureSlot;

/// The board between the relay task (on `lp-net` on the C6) and the server
/// (on the main thread): one `static` per image.
///
/// - The **server side** hands it the Cloud relay switch whenever the
///   network file changes ([`Self::settings_changed`], from the server's
///   `NetworkChanged` hook and once at boot) and the account entries
///   whenever the device store changes ([`Self::access_changed`], from the
///   `AccessChanged` hook and once at boot), and reads the relay's state
///   back for the network status ([`Self::wire_state`]) and the heartbeat.
///   **Only the server's thread touches the filesystem.** For relay
///   protocol 2 it also hands over the project's facts when they change
///   ([`Self::project_changed`]) and the pictures the relay asks for
///   ([`Self::pictures`], [`Self::picture_ready`]), both from
///   `relay_picture_source::serve_relay`.
/// - The **relay side** takes what changed ([`Self::take_cloud_relay`],
///   [`Self::take_accounts`], [`Self::take_project`], the picture slot's
///   news), publishes the driver after every pass ([`Self::publish`]), and
///   waits on [`Self::wait`].
///
/// Every access is a short critical section (a clone or a pointer move at
/// most). The account entries hold keys: they live here in RAM for the
/// relay's proof and are never logged (`RelayAccount`'s `Debug` prints
/// none). The project's facts hold its uid, a read capability: never
/// logged either (`RelayProjectFacts`' `Debug` prints none). Static RAM
/// comes out of the C6's main stack, so the facts wait here boxed (one
/// word), not inline.
pub struct RelayBoard {
    inner: Mutex<RefCell<Inner>>,
    wake: Signal<CriticalSectionRawMutex, ()>,
    /// The picture between the main thread and the relay
    /// ([`RelayPictureSlot`]).
    pub pictures: RelayPictureSlot,
}

struct Inner {
    cloud_relay: Option<bool>,
    accounts: Option<Vec<RelayAccount>>,
    /// The project's facts handed over since the last take (`Some(None)`:
    /// nothing is loaded now).
    project: Option<Box<Option<RelayProjectFacts>>>,
    state: RelayState,
    counters: RelayCounters,
    routes: usize,
    pictures: RelayPictureMode,
}

impl RelayBoard {
    /// An empty board: `off`, nothing handed over yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(Inner {
                cloud_relay: None,
                accounts: None,
                project: None,
                state: RelayState::Off,
                counters: RelayCounters {
                    rx_bytes: 0,
                    tx_bytes: 0,
                    routes: 0,
                    takeovers: 0,
                    busy: 0,
                    pictures: 0,
                },
                routes: 0,
                pictures: RelayPictureMode::Off,
            })),
            wake: Signal::new(),
            pictures: RelayPictureSlot::new(),
        }
    }

    // --- the server's side ------------------------------------------------

    /// The network file as it now stands: its Cloud relay switch.
    pub fn settings_changed(&self, file: &NetworkFile) {
        critical_section::with(|cs| {
            self.inner.borrow_ref_mut(cs).cloud_relay = Some(file.cloud_relay);
        });
        self.wake.signal(());
    }

    /// The device store as it now stands: its account entries.
    pub fn access_changed(&self, store: &DeviceAccessFile) {
        let accounts = RelayAccount::from_entries(&store.secrets);
        critical_section::with(|cs| {
            self.inner.borrow_ref_mut(cs).accounts = Some(accounts);
        });
        self.wake.signal(());
    }

    /// The server's project as it now stands (`None`: nothing loaded). Any
    /// time, whatever the relay is doing: a name and a uid, the project's
    /// identity (the board reports it after every registration).
    pub fn project_changed(&self, facts: Option<RelayProjectFacts>) {
        let facts = Box::new(facts);
        let replaced =
            critical_section::with(|cs| self.inner.borrow_ref_mut(cs).project.replace(facts));
        drop(replaced);
        self.wake.signal(());
    }

    /// The picture the relay asked for is made: hand it over and wake the
    /// relay ([`RelayPictureSlot::put_ready`]).
    pub fn picture_ready(&self, buf: Vec<u8>) {
        self.pictures.put_ready(buf);
        self.wake.signal(());
    }

    /// The relay's state, in the wire's words.
    #[must_use]
    pub fn wire_state(&self) -> WireRelayState {
        wire_relay_state(self.state())
    }

    /// The relay's state.
    #[must_use]
    pub fn state(&self) -> RelayState {
        critical_section::with(|cs| self.inner.borrow_ref(cs).state)
    }

    /// The driver's counters, how many routes it holds and what its
    /// pictures are doing, for the heartbeat's `[relay]` line.
    #[must_use]
    pub fn heartbeat(&self) -> (RelayState, RelayCounters, usize, RelayPictureMode) {
        critical_section::with(|cs| {
            let inner = self.inner.borrow_ref(cs);
            (inner.state, inner.counters, inner.routes, inner.pictures)
        })
    }

    // --- the relay's side ---------------------------------------------------

    /// The Cloud relay switch handed over since the last take.
    pub fn take_cloud_relay(&self) -> Option<bool> {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).cloud_relay.take())
    }

    /// The account entries handed over since the last take.
    pub fn take_accounts(&self) -> Option<Vec<RelayAccount>> {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).accounts.take())
    }

    /// The project's facts handed over since the last take.
    pub fn take_project(&self) -> Option<Option<RelayProjectFacts>> {
        let boxed = critical_section::with(|cs| self.inner.borrow_ref_mut(cs).project.take());
        boxed.map(|facts| *facts)
    }

    /// Publish the driver's state, counters and picture mode at `now_us`.
    pub fn publish(&self, driver: &RelayDriver, now_us: Micros) {
        let (state, counters, routes, pictures) = (
            driver.state(),
            driver.counters(),
            usize::from(driver.route().is_some()),
            driver.picture_mode(now_us),
        );
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow_ref_mut(cs);
            inner.state = state;
            inner.counters = counters;
            inner.routes = routes;
            inner.pictures = pictures;
        });
    }

    /// Wait for news from the server's side.
    pub async fn wait(&self) {
        self.wake.wait().await;
    }
}

impl Default for RelayBoard {
    fn default() -> Self {
        Self::new()
    }
}

/// The relay client's state as the wire says it (`NetworkStatus.relay`):
/// the hub's refusals folded into the three a person can act on —
/// `unknownAccount` (refresh the key), `updateFirmware` (a relay protocol
/// the hub no longer takes), and `busy` for the rest, which the board
/// retries by itself.
#[must_use]
pub fn wire_relay_state(state: RelayState) -> WireRelayState {
    match state {
        RelayState::Off => WireRelayState::Off,
        RelayState::NoAccount => WireRelayState::NoAccount,
        RelayState::WaitingForInternet => WireRelayState::WaitingForInternet,
        RelayState::Connecting => WireRelayState::Connecting,
        RelayState::Connected => WireRelayState::Connected,
        RelayState::Refused { reason } => WireRelayState::Refused {
            reason: match reason {
                RefuseReason::UnknownAccount => RelayRefusal::UnknownAccount,
                RefuseReason::VersionTooOld => RelayRefusal::UpdateFirmware,
                RefuseReason::VersionTooNew
                | RefuseReason::TooManyBoards
                | RefuseReason::Malformed
                | RefuseReason::Busy => RelayRefusal::Busy,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_access::{SecretEntry, SecretKind, Tier};

    #[test]
    fn settings_and_accounts_reach_the_relay_once_each() {
        let board = RelayBoard::new();
        assert_eq!(board.take_cloud_relay(), None);
        let mut file = NetworkFile::none();
        file.cloud_relay = false;
        board.settings_changed(&file);
        assert_eq!(board.take_cloud_relay(), Some(false));
        assert_eq!(board.take_cloud_relay(), None);

        let entry = |kind, seed| SecretEntry {
            label: String::from("x"),
            kind,
            tier: Tier::Edit,
            salt: [seed; 16],
            iterations: 1,
            k: [seed; 32],
            added_at: None,
        };
        let store = DeviceAccessFile {
            secrets: alloc::vec![
                entry(SecretKind::Password, 1),
                entry(SecretKind::Account, 2)
            ],
            ..DeviceAccessFile::default()
        };
        board.access_changed(&store);
        let accounts = board.take_accounts().expect("handed over");
        assert_eq!(accounts.len(), 1, "only account entries");
        assert_eq!(accounts[0].salt, [2; 16]);
        assert!(board.take_accounts().is_none());
        assert_eq!(board.wire_state(), WireRelayState::Off);
    }

    #[test]
    fn the_project_reaches_the_relay_once_per_change() {
        let board = RelayBoard::new();
        assert_eq!(board.take_project(), None, "nothing handed over yet");
        board.project_changed(None);
        assert_eq!(board.take_project(), Some(None), "nothing loaded");
        assert_eq!(board.take_project(), None);
        let facts = |name: &str| RelayProjectFacts {
            name: String::from(name),
            uid: Some(String::from("prj7m3qk2x9z4w8v6t5r1n0p2a4c")),
            content_hash: None,
        };
        board.project_changed(Some(facts("Basic")));
        board.project_changed(Some(facts("Porch")));
        assert_eq!(
            board.take_project(),
            Some(Some(facts("Porch"))),
            "the latest wins"
        );
        assert_eq!(board.take_project(), None);
    }

    #[test]
    fn the_hubs_refusals_fold_into_three_words() {
        let refused = |reason| wire_relay_state(RelayState::Refused { reason });
        assert_eq!(
            refused(RefuseReason::UnknownAccount),
            WireRelayState::Refused {
                reason: RelayRefusal::UnknownAccount
            }
        );
        assert_eq!(
            refused(RefuseReason::VersionTooOld),
            WireRelayState::Refused {
                reason: RelayRefusal::UpdateFirmware
            }
        );
        for reason in [
            RefuseReason::VersionTooNew,
            RefuseReason::TooManyBoards,
            RefuseReason::Malformed,
            RefuseReason::Busy,
        ] {
            assert_eq!(
                refused(reason),
                WireRelayState::Refused {
                    reason: RelayRefusal::Busy
                }
            );
        }
        assert_eq!(
            wire_relay_state(RelayState::WaitingForInternet),
            WireRelayState::WaitingForInternet
        );
    }
}
