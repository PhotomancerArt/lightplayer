//! The heartbeat's `[wifi]` and `[relay]` lines. The `[relay]` line ends
//! `· pictures N idle|watched|off` (relay protocol 2): the pictures sent
//! since boot, and what the hub has them doing now. Engine code: it runs
//! from the server loop's upkeep, never from the core.

use lpc_wire::StationState;

use super::esp_frame_device::frame_counts;
use super::station_probes::{STATION_BOARD, uses_wifi};

/// One line at the heartbeat's cadence while the board is set to use Wi-Fi:
/// the station's state, the signal and address when joined, and the frames
/// across the station interface since boot. RX drops are not counted:
/// esp-radio keeps no count of the frames it drops on a full RX queue (its
/// debug line `RX QUEUE FULL` marks each), so the desk walk reads that line
/// for N11. Display only, and never a password.
pub fn log_line() {
    if !uses_wifi() {
        return;
    }
    let (frames_in, frames_out) = frame_counts();
    match STATION_BOARD.station_state() {
        StationState::Connected {
            ssid,
            ip,
            rssi,
            host,
        } => {
            log::info!(
                "[wifi] connected to {ssid} · {rssi} dBm · {ip} ({host}) · frames in {frames_in} \
                 out {frames_out} · rx drops not counted"
            );
            #[cfg(feature = "radio_dma_diag")]
            crate::radio_dma_diag::log("joined");
        }
        state => log::info!(
            "[wifi] {} {} · frames in {frames_in} out {frames_out}",
            state.kind(),
            state.ssid().unwrap_or("")
        ),
    }
    let (relay, counters, routes, pictures) = super::relay_probes::RELAY_BOARD.heartbeat();
    log::info!(
        "[relay] state={relay} routes={routes} rx={} tx={} · takeovers {} busy {} · pictures {} {}",
        counters.rx_bytes,
        counters.tx_bytes,
        counters.takeovers,
        counters.busy,
        counters.pictures,
        pictures.word()
    );
}
