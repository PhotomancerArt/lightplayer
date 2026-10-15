//! What the relay client is told once, at start.

use alloc::string::String;

/// The board's fixed facts and where the relay is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayClientConfig {
    /// The relay's host name (`lightplayer.app`), resolved before each dial
    /// and sent as the upgrade's `Host`.
    pub host: String,
    /// The relay's port (80 on the product: the device leg is plain HTTP).
    pub port: u16,
    /// The board's MAC.
    pub board_mac: [u8; 6],
    /// The board's name for people.
    pub label: String,
    /// The device wire version the board speaks.
    pub wire_proto: u32,
    /// How many browser sessions the board holds at once (the C6: one).
    /// An `Open` past it is answered `Close { Busy }`.
    pub max_routes: usize,
    /// The version of the image the client runs in (on a split C6, the
    /// core's): the protocol 2 hello's firmware, cut to
    /// [`MAX_FIRMWARE_BYTES`](crate::MAX_FIRMWARE_BYTES) of ASCII. Empty
    /// means "unknown". Static: every board's version is its image's (the
    /// C6's manifest slot, lp-cli's `LP_APP_VERSION`), so a board holds no
    /// heap for it, and its boot heap does not move with the version's
    /// length.
    pub firmware: &'static str,
}
