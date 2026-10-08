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
//! - [`radio_link_mode`]: what this boot's radio links are for — serving the
//!   wire, or taking an update in core-only — decided once before any link
//!   opens (feature `radio-link`);
//! - [`radio_link_config`]: one link's lp-link configuration, its frame size
//!   fitted to the connection's ATT MTU, its receive window to the mode
//!   (feature `radio-link`);
//! - [`radio_link_port`]: the slots the radio side and the mux (or
//!   core-only) share — each open connection's `Link`, the boot's mode, and
//!   the signals both halves wait on (feature `radio-link`);
//! - [`slot_edge`]: which edge serves a slot's link — the network slot's
//!   two, the LAN endpoint and the relay driver, take turns (feature
//!   `radio-link`);
//! - [`parked_handshake`]: a newcomer's first frame while the network slot
//!   is held, and the same-key takeover rule (feature `wifi`);
//! - [`lan_link_config`]: one network link's lp-link configuration, the
//!   `ws()` preset cut to the board (feature `wifi`);
//! - [`link_mux_transport`]: USB plus the radio links as one server
//!   transport: whole wire messages on each link's proto channel, the hello
//!   per session, the login deadline, and channel 3 handed to the core with
//!   the link's tier (feature `radio-link`);
//! - [`radio_update_channel`]: what the mux hands the core's update hook
//!   (feature `radio-link`);
//! - [`frame_buf_holder`]: a transport letting go of the shared frame buffer
//!   before someone else serializes into it.
//!
//! See `docs/adr/2026-09-24-ble-transport.md`, and for channel 3
//! `docs/adr/2026-10-06-ota-update-protocol.md`.

#[cfg(feature = "server")]
pub mod frame_buf_holder;
pub mod hci_connection_ledger;
#[cfg(feature = "wifi")]
pub mod lan_link_config;
#[cfg(feature = "radio-link")]
pub mod link_mux_transport;
#[cfg(feature = "wifi")]
pub mod parked_handshake;
#[cfg(feature = "radio-link")]
pub mod radio_link_config;
#[cfg(feature = "radio-link")]
pub mod radio_link_mode;
#[cfg(feature = "radio-link")]
pub mod radio_link_port;
#[cfg(feature = "radio-link")]
pub mod radio_update_channel;
#[cfg(feature = "radio-link")]
pub mod slot_edge;

#[cfg(feature = "server")]
pub use frame_buf_holder::FrameBufHolder;
#[cfg(feature = "radio-link")]
pub use link_mux_transport::{
    LAN_WRITE_DEADLINE_MS, LOGIN_DEADLINE_MS, LinkMuxTransport, RADIO_WRITE_DEADLINE_MS, now_us,
};
#[cfg(feature = "wifi")]
pub use parked_handshake::{ChallengeVerdict, PARKED_FRAME_MAX, ParkRefused};
#[cfg(feature = "radio-link")]
pub use radio_link_config::{MtuTooSmall, radio_link_config, radio_max_payload};
#[cfg(feature = "radio-link")]
pub use radio_link_mode::{RadioLinkMode, UPDATE_RX_WINDOW};
#[cfg(feature = "radio-link")]
pub use radio_link_port::{
    CloseReason, LINK_SLOTS, NETWORK_LINK_SLOTS, NotHeld, OpenRefused, PortLock, RADIO_LINK_SLOTS,
    RadioLinkEvent, RadioLinkPort, RadioLinkSlot, SharedPort, SlotHeld,
};
#[cfg(feature = "radio-link")]
pub use radio_update_channel::{RadioUpdate, RadioUpdateHook};
#[cfg(feature = "radio-link")]
pub use slot_edge::SlotEdge;
