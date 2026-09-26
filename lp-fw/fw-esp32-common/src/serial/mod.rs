//! Chip-generic serial helpers.

pub mod chunked_write;
// The USB-Serial-JTAG IN-endpoint gate. Chip-free (the register touches are
// injected); the C6 and S3 io_tasks wrap their TX half in it, the classic v3
// (UART, no USB-Serial-JTAG) never names it.
pub mod in_endpoint;
// Ungated on purpose since M6 P1b, though only `server` builds have a
// heartbeat to report it in: the connection stamps are written by
// `usb_connection`, which every native-USB image links whatever else it
// carries, and the statics' demangled paths are a probe target from outside a
// running image. `current()` — the only part that needs `lpc-wire` — is still
// behind `server`.
pub mod link_counters;
// Ungated for the same reason: `usb_connection` bumps it.
pub mod link_epoch;
pub mod shared_serial;
pub mod usb_connection;

/// Wire-protocol serialization for the host link — the chip-agnostic half of
/// every firmware's `serial::io_task`.
#[cfg(feature = "server")]
pub mod server_msg;

/// One link's packed-reply state: the encoding, and the learned table that
/// lives only while a host has the link packed.
#[cfg(feature = "server")]
pub mod packed_link;
