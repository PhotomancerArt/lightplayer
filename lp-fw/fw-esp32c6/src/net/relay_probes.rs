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
//!
//! Relay protocol 2's answers come from here too: [`serve_relay`], called
//! by the server loop's frame hook on the main thread, makes the picture
//! the relay asked for and hands over the project's facts when they change
//! (`fw_esp32_common::net::relay::serve_relay`, engine-side code: the
//! relay task in the core only moves what it hands over).

use core::cell::RefCell;

use critical_section::Mutex;
use fw_esp32_common::net::relay::{RelayBoard, RelaySourceState};
use lpa_server::LpServer;
use lpc_access::DeviceAccessFile;

/// The one board of this image.
pub static RELAY_BOARD: RelayBoard = RelayBoard::new();

/// [`serve_relay`]'s state between frames (a few words; its one vector is
/// heap). Only the main thread uses it; the cell is moved out and back, so
/// no critical section spans the picture's making.
static RELAY_SOURCE: Mutex<RefCell<RelaySourceState>> =
    Mutex::new(RefCell::new(RelaySourceState::new()));

/// The frame hook's relay half: answer the relay from the server (a
/// picture when one is asked for, the project's facts when they change).
/// Every frame; when nothing is asked its cost is an atomic load and a
/// time compare, with the state moved out and back.
pub fn serve_relay(server: &LpServer) {
    let now_ms = embassy_time::Instant::now().as_millis();
    let mut state = critical_section::with(|cs| RELAY_SOURCE.borrow(cs).take());
    fw_esp32_common::net::relay::serve_relay(&RELAY_BOARD, server, now_ms, &mut state);
    critical_section::with(|cs| RELAY_SOURCE.borrow(cs).replace(state));
}

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
