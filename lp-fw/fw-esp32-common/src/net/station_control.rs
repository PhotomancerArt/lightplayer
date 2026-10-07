//! The station's radio controls, behind a LightPlayer-owned interface.

use alloc::vec::Vec;
use core::future::Future;

use lpc_wire::HeardNetwork;

/// How one connect attempt ended, as the radio saw it. The station task
/// turns it into the policy's events ([`crate::net::StationEvent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectOutcome {
    /// Associated: the password was accepted. DHCP is next.
    Associated,
    /// The network refused the password.
    AuthFailed,
    /// The station did not hear the network.
    NotHeard,
    /// The attempt ended for another reason (the radio's reason code is
    /// logged by the implementation).
    Ended,
}

/// The controls the station task drives (plan MD4): the C6 implements it
/// over esp-radio's `WifiController` (`fw-esp32c6`'s `net/esp_station.rs`),
/// and the network seam implements it again over the emulator's virtual
/// access points (Wi-Fi roadmap M6, PR C). The policy above it
/// ([`crate::net::StationPolicy`]) never sees either.
///
/// Every method is a runtime-neutral future (the sans-IO ADR's rule for
/// `async` at a seam): it may wait on the radio's own events, never on a
/// particular executor.
///
/// What an implementation must do to be equivalent to the radio:
///
/// - [`Self::scan`] listens on 2.4 GHz and answers every network heard with
///   a name (hidden ones omitted), any order, about two seconds later — or
///   `None` when the radio gave no answer, never an empty list in its place
///   (an empty list says the radio listened and heard nothing);
/// - [`Self::connect`] associates with `ssid` using `password` (empty for
///   an open network) and resolves once the attempt is decided; on
///   [`ConnectOutcome::Associated`] the frame device's link goes up;
/// - [`Self::wait_link_lost`] resolves when an associated station loses
///   the network (the link goes down), and at once when it is not
///   associated;
/// - [`Self::disconnect`] leaves (a no-op when not associated);
/// - [`Self::rssi`] is the associated network's signal in dBm.
pub trait StationControl {
    /// Listen for networks.
    fn scan(&mut self) -> impl Future<Output = Option<Vec<HeardNetwork>>>;

    /// Try to join `ssid`.
    fn connect(&mut self, ssid: &str, password: &str) -> impl Future<Output = ConnectOutcome>;

    /// Leave the network the station is on.
    fn disconnect(&mut self) -> impl Future<Output = ()>;

    /// Wait until the associated station loses its network.
    fn wait_link_lost(&mut self) -> impl Future<Output = ()>;

    /// The associated network's signal in dBm, when there is one.
    fn rssi(&self) -> Option<i8>;
}
