//! The board between the relay task and the server, and the server's hooks
//! and probe over it (the station's twin: `station_probes`).
//!
//! The relay task runs on `lp-net`; the server on the main thread. They
//! meet only at [`RELAY_BOARD`] (`fw_esp32_common::net::relay::RelayBoard`):
//! short critical sections, no waiting. Only the main thread touches the
//! filesystem — the Cloud relay switch and the account entries reach the
//! relay through the board, at boot ([`boot_access`], and the network file
//! through `station_probes::boot_settings`) and after every change
//! ([`access_changed`], `station_probes::network_changed`).

use fw_esp32_common::net::relay::RelayBoard;
use lpc_access::DeviceAccessFile;

/// The one board of this image.
pub static RELAY_BOARD: RelayBoard = RelayBoard::new();

/// The device store as `core_boot` read it: the relay's first account
/// entries.
pub fn boot_access(store: &DeviceAccessFile) {
    RELAY_BOARD.access_changed(store);
}

/// The server's `AccessChanged` hook: the device store as it now stands
/// (a key Studio installed over USB reaches the relay at once).
pub fn access_changed(store: &DeviceAccessFile) {
    RELAY_BOARD.access_changed(store);
}

/// The server's `RelayProbe`.
pub fn relay_probe() -> lpc_wire::RelayState {
    RELAY_BOARD.wire_state()
}
