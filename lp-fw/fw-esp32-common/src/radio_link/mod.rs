//! Radio links: the chip-generic half of carrying the wire over a radio.
//!
//! The chip crate owns the radio stack (fw-esp32c6's `ble` module); this
//! module owns everything about a radio link that is not a radio fact, so it
//! is host-tested here and shared by any chip that grows one:
//!
//! - [`line_joiner`]: written chunks → `M!` lines (host → board);
//! - [`line_chunker`]: one framed line → notify-sized values (board → host);
//! - [`radio_link_port`]: the channels the radio side and the mux meet on;
//! - [`link_mux_transport`]: USB plus the radio links as one server transport.
//!
//! See `docs/adr/2026-09-24-ble-transport.md`.

pub mod line_chunker;
pub mod line_joiner;
#[cfg(feature = "server")]
pub mod link_mux_transport;
#[cfg(feature = "server")]
pub mod radio_link_port;

#[cfg(feature = "server")]
pub use link_mux_transport::{LOGIN_DEADLINE_MS, LinkMuxTransport, RADIO_WRITE_DEADLINE_MS};
#[cfg(feature = "server")]
pub use radio_link_port::{
    CloseReason, RADIO_LINK_PORT, RADIO_LINK_SLOTS, RadioLinkEvent, RadioLinkPort, RadioLinkSlot,
    RadioWriteRequest,
};

/// The longest `M!` line a radio link accepts from the host: the wire's frame
/// budget plus its serialization margin — the same bound the board's own
/// frame buffer is sized to.
#[cfg(feature = "server")]
pub const RADIO_LINE_CAP: usize = lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES;
