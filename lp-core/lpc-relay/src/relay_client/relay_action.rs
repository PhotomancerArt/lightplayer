//! What the relay client asks its edge to do.

use alloc::string::String;
use alloc::vec::Vec;

/// One instruction for the edge (the firmware's relay task, lp-cli's host
/// board), returned in order by [`RelayClient::handle`](super::RelayClient::handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayAction {
    /// Resolve `host` to an IPv4 address; answer with
    /// [`RelayEvent::Resolved`](super::RelayEvent::Resolved).
    Resolve { host: String },
    /// Open the device leg: TCP to `addr:port`, then the WebSocket upgrade
    /// on [`RELAY_DEVICE_PATH`](crate::RELAY_DEVICE_PATH) with the
    /// configured host as `Host`. Answer with
    /// [`RelayEvent::Connected`](super::RelayEvent::Connected) or
    /// [`RelayEvent::Closed`](super::RelayEvent::Closed).
    Connect { addr: [u8; 4], port: u16 },
    /// Send one binary WebSocket message on the device leg.
    Send(Vec<u8>),
    /// Close the device leg (a WebSocket close, then the socket). No
    /// `Closed` event is expected after it.
    Close,
    /// A browser session opened on `route`: start a secure lp-link
    /// responder for it.
    RouteOpened(u16),
    /// One lp-link frame arrived on `route`: hand it to that route's link.
    RouteFrame { route: u16, bytes: Vec<u8> },
    /// `route` is gone (its browser left, or the device leg closed): drop
    /// that route's link.
    RouteClosed(u16),
    /// Take a picture of the board's lamps now. Answer with
    /// [`RelayEvent::PictureReady`](super::RelayEvent::PictureReady) when
    /// it is taken, or never (a core with no engine has none). The
    /// picture's bytes never pass through the client.
    TakePicture,
    /// Send the picture you hold on the device leg, as one
    /// [`RelayFrame::Picture`](crate::RelayFrame::Picture) message, encoded
    /// where its buffer is.
    SendPicture,
    /// The picture you hold is not wanted (the leg went away, or nobody
    /// asked); keep its buffer if you like.
    DropPicture,
}
