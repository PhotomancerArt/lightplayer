//! The BLE link: the wire, on lp-link, over a Nordic-UART GATT service.
//!
//! Plan `ble-remote-control` M4; the decision record is
//! `docs/adr/2026-09-24-ble-transport.md`. In one paragraph: BLE is on
//! unless the device store turns it off (`/.lp/access.json`, `bleEnabled`),
//! read once at boot — an emulated C6 starts the controller and advertises,
//! but no central ever connects to it. When on, each connection is one
//! *untrusted* radio link into the
//! link mux (`fw_esp32_common::radio_link`), which the server's access gate
//! treats as it treats any untrusted link. BLE never exposes flashing: there
//! is no such request on the wire.
//!
//! - [`ble_task`]: bring-up, the host runner, advertising, connection tasks;
//! - [`ble_connection`]: one connection's life as a link;
//! - [`nus_service`]: the GATT server (the spike's UUIDs);
//! - [`advertising`]: address, name, advertisement;
//! - [`conn_params`]: the 15 ms / 4 s request and its read-back;
//! - [`notify_queue`]: board → host frames, one per notification, and what
//!   trouble-host queues.
//!
//! Each connection's wire is an lp-link session (plan `ble-on-lp-link`): one
//! frame per ATT write or notification, sized to the connection's MTU. The
//! pure halves — the link itself, its sizing, the mux that turns its
//! messages into server requests and replies — live in
//! `fw_esp32_common::radio_link`, where they are host-tested.

mod advertising;
mod ble_connection;
mod ble_task;
mod conn_params;
mod hci_transport;
mod notify_queue;
mod nus_service;

pub use advertising::refresh_advertised_name;
pub use ble_task::start;

/// Run K's parameter file (`desk_ble_params`).
#[cfg(feature = "desk_ble_params")]
pub fn desk_params_path() -> &'static str {
    conn_params::desk::PATH
}

/// Take Run K's parameters from the file's text (`desk_ble_params`).
#[cfg(feature = "desk_ble_params")]
pub fn configure_desk_params(text: &str) {
    conn_params::desk::configure(text);
}
