//! Network bring-up's one choice: the radio, or the network seam (plan P10,
//! the switch shape of the seams ADR §3).
//!
//! [`choose`] reads the network seam's engaged byte
//! (`fw_esp32_common::seams::net::net_mac`) once. On silicon it reads 0 and
//! the radio runs, exactly as before the seam existed: esp-radio's station
//! interface under the IP stack and [`EspStation`] driving it. On an emulated
//! board that engaged `net=lan` it reads 1: the seam's frame device and
//! station plug in under the same IP stack, the wake line is bound, and the
//! radio's controller is kept (it still runs ESP-NOW). BLE is not touched
//! either way.

use alloc::boxed::Box;

use esp_radio::wifi::{Interface, WifiController};
use fw_esp32_common::net::{NetFrameDevice, SeamFrameDevice, SeamStation};
use fw_esp32_common::seams::net::net_mac;

use super::c6_station::C6Station;
use super::esp_frame_device::{C6FrameDevice, Counted};
use super::esp_station::EspStation;

/// Build the frame device and the station for this boot. Runs on `lp-net`,
/// before its executor starts (whatever the wake wakes runs there).
pub fn choose(
    controller: WifiController<'static>,
    interface: Interface<'static>,
) -> (C6FrameDevice, C6Station) {
    if net_mac::engaged() {
        if let Some(mac) = fw_esp32_common::net::seam_frame_device::station_mac() {
            log::info!("[wifi] network: the emulator's network seam");
            crate::seams::seam_wake_handler::bind();
            // The radio's station interface (a handle) is not used on this
            // arm; the controller is (ESP-NOW).
            let _ = interface;
            return (
                NetFrameDevice::Seam(Counted(SeamFrameDevice::new(mac))),
                C6Station::Seam {
                    station: Box::new(SeamStation::new()),
                    _controller: controller,
                },
            );
        }
        log::warn!("[wifi] network seam engaged but it gave no MAC: running the radio");
    }
    log::info!("[wifi] network: the radio");
    (
        NetFrameDevice::Radio(Counted(interface)),
        C6Station::Radio(EspStation::new(controller)),
    )
}
