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

/// The longest firmware version a protocol 2 hello carries, in bytes of
/// ASCII: the firmware's own version slot is 40 bytes
/// (`lpc_model::manifest::VERSION_SLOT_BYTES`). The board's hello cuts to
/// it ([`RelayHello::with_firmware`](crate::RelayHello::with_firmware));
/// the hub's decoder refuses a longer one.
pub const MAX_FIRMWARE_BYTES: usize = 40;

/// The longest project name a [`RelayProject`](crate::RelayProject)
/// carries, in bytes of UTF-8. The board cuts to it; the hub's decoder
/// refuses a longer one.
pub const MAX_PROJECT_NAME_BYTES: usize = 32;

/// Each project tag's length: the first 16 bytes of an HMAC-SHA256
/// ([`crate::relay_project`]). Fixed by the encoding; both ends.
pub const PROJECT_TAG_BYTES: usize = 16;

/// The most outputs a [`RelayPicture`](crate::RelayPicture) carries. The
/// hub's decoder refuses a picture with more; a board with more outputs
/// sends its first sixteen, in tree order.
pub const MAX_PICTURE_OUTPUTS: usize = 16;

/// How many colours a board sends at most today (768 bytes; a frame of at
/// most 836). **Not a protocol limit**: the hub takes any count that fits a
/// [`MAX_RELAY_FRAME`] (at most 660 with sixteen outputs), so a later
/// firmware can send more with no protocol change. The board keeps to it.
pub const DEFAULT_PICTURE_SAMPLES: usize = 256;

/// The board's floor on [`PictureRate::watched_ms`](crate::PictureRate):
/// a hub bug can never push a board past four pictures a second. Enforced
/// by the board ([`PictureRate::clamped`](crate::PictureRate::clamped)).
pub const MIN_WATCHED_MS: u16 = 250;

/// The board's floor on a non-zero
/// [`PictureRate::idle_s`](crate::PictureRate) (0 stays "none"). Enforced
/// by the board ([`PictureRate::clamped`](crate::PictureRate::clamped)).
pub const MIN_IDLE_S: u16 = 10;

/// The board's ceiling on [`PictureRate::idle_s`](crate::PictureRate).
/// Enforced by the board ([`PictureRate::clamped`](crate::PictureRate::clamped)).
pub const MAX_IDLE_S: u16 = 3600;

/// The board's cap on
/// [`PictureRate::watched_for_s`](crate::PictureRate): however long the hub
/// asks, a board counts as watched for at most five minutes from the last
/// rate it heard, so a lost frame or a hub restart can never leave it fast
/// for long. Enforced by the board
/// ([`PictureRate::clamped`](crate::PictureRate::clamped)).
pub const MAX_WATCHED_FOR_S: u16 = 300;
