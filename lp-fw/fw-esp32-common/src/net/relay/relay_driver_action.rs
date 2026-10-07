//! What the relay driver asks its edge (the C6's relay task, the host
//! harness's relay thread) to do.

use alloc::string::String;
use alloc::vec::Vec;

use crate::radio_link::RadioLinkEvent;

/// One instruction for the edge, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayDriverAction {
    /// Resolve `host` to an IPv4 address; answer with
    /// `RelayEvent::Resolved`.
    Resolve { host: String },
    /// Open the device leg: TCP to `addr:port`, then the WebSocket upgrade
    /// on `lpc_relay::RELAY_DEVICE_PATH` with the configured host (and
    /// `:port` off 80) as `Host`. Answer with `RelayEvent::Connected` or
    /// `RelayEvent::Closed`.
    Connect { addr: [u8; 4], port: u16 },
    /// Send one binary WebSocket message on the device leg.
    Send(Vec<u8>),
    /// Close the device leg.
    Close,
    /// Tell the mux, through the port (`RadioLinkPort::announce`).
    Announce(RadioLinkEvent),
}
