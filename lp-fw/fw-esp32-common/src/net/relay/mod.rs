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
//!   and asks.

pub mod relay_board;
pub mod relay_driver;
pub mod relay_driver_action;
pub mod relay_leg;
pub mod relay_route_link;

pub use relay_board::{RelayBoard, wire_relay_state};
pub use relay_driver::{CHALLENGE_WAIT_US, RelayCounters, RelayDriver};
pub use relay_driver_action::RelayDriverAction;
pub use relay_leg::{
    CONNECT_TIMEOUT_US, RelayLegBuffers, RelayLegExit, RelayLegIo, run_relay_leg,
    wait_until_may_dial,
};
pub use relay_route_link::{RelayRouteLink, RouteSlotState};
