//! The board between the station and the server, and the server's probes
//! over it.
//!
//! The station runs on the `lp-net` thread; the server and its probes on
//! the main one. They meet only at [`STATION_BOARD`]
//! (`fw_esp32_common::net::StationBoard`): short critical sections, no
//! waiting. Only the main thread touches the filesystem — the network file
//! reaches the station through the board, at boot ([`boot_settings`]) and
//! after every change ([`network_changed`]).

use lpc_access::NetworkFile;
use lpc_wire::{LastAttempt, NetworkScan, StationState};

use fw_esp32_common::net::StationBoard;

/// The one board of this image.
pub static STATION_BOARD: StationBoard = StationBoard::new();

/// The board is set to use Wi-Fi (switch on, a network saved): the Radio
/// node rule's predicate, readable from any context.
pub fn uses_wifi() -> bool {
    STATION_BOARD.uses_wifi()
}

/// The network file as `core_boot` read it, before the engine (and any
/// Radio node) exists: "set to use Wi-Fi" is decided before a project
/// opens its endpoints. The relay takes its Cloud relay switch from it too.
pub fn boot_settings(file: &NetworkFile) {
    STATION_BOARD.settings_changed(file);
    super::relay_probes::RELAY_BOARD.settings_changed(file);
}

/// The server's `NetworkChanged` hook: the file as it now stands, for the
/// station and the relay (the Cloud relay switch).
pub fn network_changed(file: &NetworkFile) {
    STATION_BOARD.settings_changed(file);
    super::relay_probes::RELAY_BOARD.settings_changed(file);
}

/// The server's `StationProbe`.
pub fn station_probe() -> StationState {
    STATION_BOARD.station_state()
}

/// The server's `LastAttemptProbe`.
pub fn last_attempt_probe(ssid: &str) -> Option<LastAttempt> {
    STATION_BOARD.last_attempt(ssid)
}

/// The server's `ScanProbe`: what the radio heard in the last 10 s, or
/// `scanning` (and the station listens).
pub fn scan_probe() -> NetworkScan {
    STATION_BOARD.scan_answer(embassy_time::Instant::now().as_millis())
}
