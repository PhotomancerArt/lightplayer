//! USB-Serial-JTAG cable monitor for ESP32-S3 — the chip half.
//!
//! What a SOF sample means lives in
//! [`fw_esp32_common::serial::usb_connection`], which is chip-free and
//! host-tested. What is genuinely an S3 fact is here: reading and clearing
//! `USB_DEVICE.int_raw.sof`, plus the two IN-endpoint register touches the
//! USB link task's gate injects ([`UsbSerialJtagInEndpoint`]). The C6 keeps
//! the same facts for the same reason.
//!
//! The S3 has no `spike_uart0_link` build, so its link is always the real USB
//! one and SOF always gates writes.

use fw_esp32_common::serial::usb_connection::UsbLinkState;

pub struct UsbConnectionMonitor {
    link: UsbLinkState,
}

impl UsbConnectionMonitor {
    pub fn new() -> Self {
        Self {
            link: UsbLinkState::new(false),
        }
    }

    /// Sample the SOF raw interrupt bit and update internal state. The link
    /// task calls this at most every 2 ms (see `UsbLinkState`).
    pub fn poll(&mut self) {
        let regs = esp_hal::peripherals::USB_DEVICE::regs();
        let sof_received = regs.int_raw().read().sof().bit_is_set();
        regs.int_clr().write(|w| w.sof().clear_bit_by_one());
        self.link.poll_with(sof_received);
    }

    /// A USB host enumerates the board.
    pub fn is_enumerated(&self) -> bool {
        self.link.is_enumerated()
    }
}

/// This chip's USB-Serial-JTAG register touches for the link task's
/// IN-endpoint gate ([`fw_esp32_common::serial::in_endpoint`]).
pub struct UsbSerialJtagInEndpoint;

impl fw_esp32_common::serial::in_endpoint::InEndpointRegs for UsbSerialJtagInEndpoint {
    #[inline]
    fn in_ep_free() -> bool {
        esp_hal::peripherals::USB_DEVICE::regs()
            .ep1_conf()
            .read()
            .serial_in_ep_data_free()
            .bit_is_set()
    }

    #[inline]
    fn clear_serial_in_empty() {
        esp_hal::peripherals::USB_DEVICE::regs()
            .int_clr()
            .write(|w| w.serial_in_empty().clear_bit_by_one());
    }
}
