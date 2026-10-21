//! The BLE link: the wire, on lp-link, over a Nordic-UART GATT service.
//!
//! The link itself is `fw_esp32_radio::ble` (bring-up, the host runner,
//! advertising, each connection's link; the decision record is
//! `docs/adr/2026-09-24-ble-transport.md`). What stays here is this chip's:
//! the controller's configuration ([`ble_controller_config`], NPL setters
//! only this controller family has), and the rule that the XIAO-C6's RF
//! switch is set before the controller starts ([`start`] takes the proof).

mod ble_controller_config;

use fw_esp32_common::radio_link::RadioLinkPort;
pub use fw_esp32_radio::ble::refresh_advertised_name;
#[cfg(feature = "desk_ble_params")]
pub use fw_esp32_radio::ble::{configure_desk_params, desk_params_path};

use crate::board::esp32c6::board_quirks::BoardQuirksApplied;

/// Bring BLE up and start serving it on `port`, with this chip's controller
/// configuration. Needs the RF-switch quirk to have run first (`_quirks` is
/// the proof).
pub fn start(
    spawner: embassy_executor::Spawner,
    bt: esp_hal::peripherals::BT<'static>,
    port: &'static RadioLinkPort,
    _quirks: BoardQuirksApplied,
) {
    fw_esp32_radio::ble::start(
        spawner,
        bt,
        port,
        ble_controller_config::ble_controller_config(),
    );
}
