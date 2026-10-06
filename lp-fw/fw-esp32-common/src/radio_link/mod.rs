//! Radio links: the chip-generic half of carrying the wire over a radio.
//!
//! The chip crate owns the radio stack (fw-esp32c6's `ble` module); this
//! module owns everything about a radio link that is not a radio fact, so it
//! is host-tested here and shared by any chip that grows one. Each radio
//! connection is one lp-link session (plan `ble-on-lp-link`,
//! `docs/adr/2026-09-27-lp-link-one-comms-layer.md`): Datagram framing, one
//! frame per ATT write or notification, never an ATT long write.
//!
//! - [`hci_connection_ledger`]: which BLE connections the controller holds
//!   open, so a controller reset can close them in the host too;
//! - [`radio_link_config`]: one link's lp-link configuration, its frame size
//!   fitted to the connection's ATT MTU (feature `radio-link`);
//! - [`radio_link_port`]: the slots the radio side and the mux share — each
//!   open connection's `Link`, and the signals both halves wait on (feature
//!   `radio-link`);
//! - [`link_mux_transport`]: USB plus the radio links as one server
//!   transport: whole wire messages on each link's proto channel, the hello
//!   per session, the login deadline (feature `radio-link`);
//! - [`frame_buf_holder`]: a transport letting go of the shared frame buffer
//!   before someone else serializes into it.
//!
//! See `docs/adr/2026-09-24-ble-transport.md`.

#[cfg(feature = "server")]
pub mod frame_buf_holder;
pub mod hci_connection_ledger;
#[cfg(feature = "radio-link")]
pub mod link_mux_transport;
#[cfg(feature = "radio-link")]
pub mod radio_link_config;
#[cfg(feature = "radio-link")]
pub mod radio_link_port;

#[cfg(feature = "server")]
pub use frame_buf_holder::FrameBufHolder;
#[cfg(feature = "radio-link")]
pub use link_mux_transport::{
    LOGIN_DEADLINE_MS, LinkMuxTransport, RADIO_WRITE_DEADLINE_MS, now_us,
};
#[cfg(feature = "radio-link")]
pub use radio_link_config::{MtuTooSmall, radio_link_config, radio_max_payload};
#[cfg(feature = "radio-link")]
pub use radio_link_port::{
    CloseReason, RADIO_LINK_SLOTS, RadioLinkEvent, RadioLinkPort, RadioLinkSlot,
};
