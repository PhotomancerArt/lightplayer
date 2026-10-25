//! Network bring-up's one choice: the radio, or the network seam (plan P10,
//! the switch shape of the seams ADR §3).
//!
//! [`choose`] reads the network seam's engaged byte
//! (`fw_esp32_common::seams::net::net_mac`) once. On silicon it reads 0 and
//! the radio runs, exactly as before the seam existed: esp-radio's station
//! interface under the IP stack and [`EspStation`] driving it. On an emulated
//! board that engaged `net=lan` it reads 1: the seam's frame device and
//! station plug in under the same IP stack, the wake line is bound (the
//! chip's [`NetChip::bind_seam_wake`]), and the radio's controller is kept
//! (it still runs ESP-NOW). BLE is not touched either way. The seam is
//! RISC-V and Xtensa only (the C6 and the S3); a host build runs the radio
//! arm.

#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
use alloc::boxed::Box;

use esp_radio::wifi::{Interface, WifiController};
use fw_esp32_common::net::NetFrameDevice;
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
use fw_esp32_common::net::{SeamFrameDevice, SeamStation};
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
use fw_esp32_common::seams::net::net_mac;

use super::board_station::BoardStation;
use super::esp_frame_device::{BoardFrameDevice, Counted};
use super::esp_station::EspStation;
use super::net_thread::NetChip;

/// Build the frame device and the station for this boot. Runs on `lp-net`,
/// before its executor starts (whatever the wake wakes runs there).
pub fn choose(
    controller: WifiController<'static>,
    interface: Interface<'static>,
    chip: &NetChip,
) -> (BoardFrameDevice, BoardStation) {
    #[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
    if net_mac::engaged() {
        if let Some(mac) = fw_esp32_common::net::seam_frame_device::station_mac() {
            log::info!("[wifi] network: the emulator's network seam");
            (chip.bind_seam_wake)();
            // The radio's station interface (a handle) is not used on this
            // arm; the controller is (ESP-NOW).
            let _ = interface;
            return (
                NetFrameDevice::Seam(Counted(SeamFrameDevice::new(mac))),
                BoardStation::Seam {
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
        BoardStation::Radio(EspStation::new(controller, chip.max_tx_power_qdbm)),
    )
}
