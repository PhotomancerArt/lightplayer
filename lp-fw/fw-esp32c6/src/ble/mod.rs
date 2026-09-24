//! The BLE link: the wire, unchanged, over a Nordic-UART GATT service.
//!
//! Plan `ble-remote-control` M4; the decision record is
//! `docs/adr/2026-09-24-ble-transport.md`. In one paragraph: BLE is **off
//! until the device store enables it** (`/.lp/access.json`, `bleEnabled`),
//! read once at boot — the emulated C6 never has it, so no emulator run meets
//! BLE init. When on, each connection is one *untrusted* radio link into the
//! link mux (`fw_esp32_common::radio_link`), which the server's access gate
//! treats as it treats any untrusted link. BLE never exposes flashing: there
//! is no such request on the wire.
//!
//! - [`ble_task`]: bring-up, the host runner, advertising, connection tasks;
//! - [`ble_connection`]: one connection's life as a link;
//! - [`nus_service`]: the GATT server (the spike's UUIDs);
//! - [`advertising`]: address, name, advertisement;
//! - [`conn_params`]: the 15 ms / 4 s request and its read-back;
//! - [`notify_queue`]: board → host frames, and what trouble-host queues.
//!
//! The pure halves — re-joining written chunks into lines, and chunking a
//! frame into notify values — live in `fw_esp32_common::radio_link`, where
//! they are host-tested.

mod advertising;
mod ble_connection;
mod ble_task;
mod conn_params;
mod notify_queue;
mod nus_service;

pub use advertising::refresh_advertised_name;
pub use ble_task::start;
