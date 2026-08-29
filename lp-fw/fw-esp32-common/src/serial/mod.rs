//! Chip-generic serial helpers.

pub mod chunked_write;
#[cfg(feature = "server")]
pub mod link_counters;
pub mod shared_serial;

/// Wire-protocol serialization for the host link — the chip-agnostic half of
/// every firmware's `serial::io_task`.
#[cfg(feature = "server")]
pub mod server_msg;
