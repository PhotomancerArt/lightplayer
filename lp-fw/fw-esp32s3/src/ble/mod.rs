//! The BLE link on the S3: the wire, on lp-link, over a Nordic-UART GATT
//! service (feature `ble`; S3 radio roadmap M3).
//!
//! The link itself is `fw_esp32_radio::ble`, the C6's since
//! `ble-remote-control` M4 (the decision record is
//! `docs/adr/2026-09-24-ble-transport.md`): bring-up, the host runner,
//! advertising, one untrusted lp-link per connection into the link mux. What
//! is this chip's is the controller: BTDM, not the C6's NPL, so its
//! configuration is its own ([`ble_controller_config`]). The S3 boards have
//! no RF switch to set first, so there is no board quirk to prove.

mod ble_controller_config;

use fw_esp32_common::radio_link::RadioLinkPort;
pub use fw_esp32_radio::ble::refresh_advertised_name;

/// The BT peripheral, taken from the singleton in `init_board` and handed to
/// [`start`] once the device store has been read (the C6's `take_bt`).
static BT: critical_section::Mutex<core::cell::RefCell<Option<esp_hal::peripherals::BT<'static>>>> =
    critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep the BT peripheral for [`start`]. Called from `init_board`, the one
/// place the peripheral singleton is split.
pub fn stash(bt: esp_hal::peripherals::BT<'static>) {
    critical_section::with(|cs| BT.borrow_ref_mut(cs).replace(bt));
}

/// Bring BLE up and start serving it on `port`, with this chip's controller
/// configuration. `false` (and logged) when the peripheral is gone.
pub fn start(spawner: embassy_executor::Spawner, port: &'static RadioLinkPort) -> bool {
    let Some(bt) = critical_section::with(|cs| BT.borrow_ref_mut(cs).take()) else {
        log::error!("[ble] enabled, but the BT peripheral is gone — BLE off");
        return false;
    };
    fw_esp32_radio::ble::start(
        spawner,
        bt,
        port,
        ble_controller_config::ble_controller_config(),
    );
    true
}
