//! USB-Serial-JTAG cable monitor for ESP32-C6 — the chip half.
//!
//! What a SOF sample means lives in
//! [`fw_esp32_common::serial::usb_connection`], which is chip-free and
//! host-tested. What is genuinely a C6 fact is here: reading and clearing
//! `USB_DEVICE.int_raw.sof`, plus the two IN-endpoint register touches the
//! USB link task's gate injects ([`UsbSerialJtagInEndpoint`]). The S3 keeps
//! the same facts for the same reason.

use core::sync::atomic::{AtomicBool, Ordering};

use fw_esp32_common::serial::usb_connection::UsbLinkState;

/// Whether a USB host is enumerating this device, as the link task last saw
/// it.
///
/// Starts `true`: until the monitor has polled, "a host might be there" is
/// the safe answer, because the one consumer — the power button's switch
/// mode — must not drop the link into deep sleep before anyone has looked.
static HOST_ENUMERATED: AtomicBool = AtomicBool::new(true);

/// Whether a USB host (Studio, a computer) is on the USB-Serial-JTAG port
/// right now. A charger or power bank sends no SOF, so it does not count.
/// Read only by the product's power platform (`hardware::power`).
#[cfg(all(not(fw_harness), feature = "server"))]
pub fn host_enumerated() -> bool {
    HOST_ENUMERATED.load(Ordering::Relaxed)
}

pub struct UsbConnectionMonitor {
    link: UsbLinkState,
}

impl UsbConnectionMonitor {
    pub fn new() -> Self {
        Self {
            // The `spike_uart0_link` build moves the host link to UART0,
            // which has no cable to detect: SOF never arrives there and must
            // not gate writes.
            link: UsbLinkState::new(cfg!(feature = "spike_uart0_link")),
        }
    }

    /// Sample the SOF raw interrupt bit and update internal state. The link
    /// task calls this at most every 2 ms (see `UsbLinkState`).
    pub fn poll(&mut self) {
        let regs = esp_hal::peripherals::USB_DEVICE::regs();
        let sof_received = regs.int_raw().read().sof().bit_is_set();
        regs.int_clr().write(|w| w.sof().clear_bit_by_one());
        self.link.poll_with(sof_received);
        HOST_ENUMERATED.store(self.link.is_enumerated(), Ordering::Relaxed);
    }

    /// A USB host enumerates the board (or the link is not USB at all).
    pub fn is_enumerated(&self) -> bool {
        self.link.is_enumerated()
    }
}

/// This chip's USB-Serial-JTAG register touches for the link task's
/// IN-endpoint gate ([`fw_esp32_common::serial::in_endpoint`]).
#[cfg(not(feature = "spike_uart0_link"))]
pub struct UsbSerialJtagInEndpoint;

#[cfg(not(feature = "spike_uart0_link"))]
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
