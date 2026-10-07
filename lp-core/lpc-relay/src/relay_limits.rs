//! The relay's limits, shared by the hub and the boards.

/// The largest relay frame, in bytes: one device-leg WebSocket message. An
/// lp-link frame on the LAN preset is at most 1,088 bytes, and
/// [`RelayFrame::Frame`](crate::RelayFrame::Frame) adds three; the rest is
/// headroom. The hub closes a socket that sends a bigger one.
pub const MAX_RELAY_FRAME: usize = 2048;

/// The most account salts a [`RelayHello`](crate::RelayHello) carries. The
/// `Registered` answer names which ones verified as a one-byte bitmask.
pub const MAX_HELLO_ACCOUNTS: usize = 8;

/// The longest board name a hello carries, in bytes of UTF-8.
pub const MAX_LABEL_BYTES: usize = 32;

/// The most routes the hub holds open to one board at once. A board takes
/// fewer if it has less room (the C6 takes one) and answers the rest
/// `Close { Busy }`.
pub const MAX_ROUTES_PER_BOARD: usize = 4;

/// How often each leg is pinged, in seconds (the WebSocket's own ping).
/// Under fly's proxy idle timeout and most home NATs' TCP timeouts.
pub const PING_INTERVAL_S: u16 = 25;

/// A leg that has heard nothing — not a frame, not a ping — for this long
/// is closed, by the hub and by the board alike.
pub const SILENT_CLOSE_S: u16 = 60;
