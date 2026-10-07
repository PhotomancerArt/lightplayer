//! What the relay client is told.

use alloc::vec::Vec;

use super::relay_account::RelayAccount;
use crate::lan_address::LanAddress;

/// One fact for [`RelayClient::handle`](super::RelayClient::handle), with
/// the time it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayEvent<'a> {
    /// The station joined a network and has an address (`true`), or lost
    /// it (`false`).
    Network { joined: bool },
    /// The board's Cloud relay switch (`/.lp/network.json`'s `cloudRelay`).
    CloudRelay(bool),
    /// The board's account entries, in store order (at start and after
    /// every change to the access store).
    Accounts(Vec<RelayAccount>),
    /// The board's LAN address, or none.
    Lan(Option<LanAddress>),
    /// The answer to [`RelayAction::Resolve`](super::RelayAction::Resolve):
    /// an address, or `None` when the name did not resolve.
    Resolved(Option<[u8; 4]>),
    /// The device leg is open (TCP up, WebSocket upgraded).
    Connected,
    /// The device leg closed, or the dial failed. `going_away` when the hub
    /// closed it with WebSocket code 1001 (a deploy).
    Closed { going_away: bool },
    /// One binary WebSocket message from the hub.
    Message(&'a [u8]),
    /// The leg heard something that is not a message (a ping): it is alive.
    Heard,
    /// The board's link on `route` has a frame to send.
    RouteSend { route: u16, bytes: &'a [u8] },
    /// The board closes `route` (its session ended).
    RouteClose { route: u16 },
    /// Time passed; deadlines are checked against `now_ms`.
    Tick,
}
