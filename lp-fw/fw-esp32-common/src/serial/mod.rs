//! Chip-generic serial helpers.

pub mod chunked_write;
// The USB-Serial-JTAG IN-endpoint gate. Chip-free (the register touches are
// injected); the C6 and S3 USB link tasks wrap their TX half in it, the
// classic v3 (UART, no USB-Serial-JTAG) never names it.
pub mod in_endpoint;
pub mod shared_serial;
pub mod usb_connection;

/// The static frame buffer every server write path shares.
#[cfg(feature = "server")]
pub mod server_msg;

/// A server message as one lp-link proto-channel payload (every board link:
/// the C6/S3 USB link, the classic's UART link, the C6's radio links).
#[cfg(feature = "server")]
pub mod server_payload;

/// One link's packed-reply state: the encoding, and the learned table that
/// lives only while a host has the link packed.
#[cfg(feature = "server")]
pub mod packed_link;
