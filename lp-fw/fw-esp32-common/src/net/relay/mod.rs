//! The board's end of the cloud relay (Wi-Fi relay plan P6; decision record
//! `docs/adr/2026-10-06-cloud-relay.md`):
//!
//! - [`relay_driver`]: the sans-IO glue between `lpc-relay`'s board client
//!   and the network slot — a route the hub opens becomes a relayed network
//!   link, or a challenge for the slot when a LAN session holds it;
//! - [`relay_leg`]: the device leg's async loop over a platform's sockets
//!   ([`RelayLegIo`]), shared by the C6's relay task
//!   (`fw-esp32c6/src/net/relay_task.rs`) and the host harness;
//! - [`relay_route_link`], [`relay_driver_action`]: what the driver keeps
//!   and asks;
//! - [`relay_board`]: what the relay task and the main thread share;
//! - [`relay_picture_slot`], [`relay_picture_source`], [`relay_picture_mode`]:
//!   relay protocol 2's pictures and project report — the hand-off between
//!   the threads, the main thread's answer from the server (engine side),
//!   and the heartbeat's words.

pub mod relay_board;
pub mod relay_driver;
pub mod relay_driver_action;
pub mod relay_leg;
pub mod relay_picture_mode;
pub mod relay_picture_slot;
pub mod relay_picture_source;
pub mod relay_route_link;

pub use relay_board::{RelayBoard, wire_relay_state};
pub use relay_driver::{CHALLENGE_WAIT_US, RelayCounters, RelayDriver};
pub use relay_driver_action::RelayDriverAction;
pub use relay_leg::{CONNECT_TIMEOUT_US, RelayLegIo, RelayLegSizes, run_relay_leg};
pub use relay_picture_mode::RelayPictureMode;
pub use relay_picture_slot::RelayPictureSlot;
pub use relay_picture_source::{RelaySourceState, serve_relay};
pub use relay_route_link::{RelayRouteLink, RouteSlotState};
