//! Chip-generic serial helpers.

pub mod chunked_write;
// Ungated on purpose since M6 P1b, though only `server` builds have a
// heartbeat to report it in: the connection stamps are written by
// `usb_connection`, which every native-USB image links whatever else it
// carries, and the statics' demangled paths are a probe target from outside a
// running image. `current()` — the only part that needs `lpc-wire` — is still
// behind `server`.
pub mod link_counters;
pub mod shared_serial;
pub mod usb_connection;

/// Wire-protocol serialization for the host link — the chip-agnostic half of
/// every firmware's `serial::io_task`.
#[cfg(feature = "server")]
pub mod server_msg;
