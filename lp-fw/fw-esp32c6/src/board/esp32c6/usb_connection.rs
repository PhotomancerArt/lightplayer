//! USB-Serial-JTAG connection monitor for ESP32-C6 — the chip half.
//!
//! The state machine, its two thresholds and its two log lines live in
//! [`fw_esp32_common::serial::usb_connection`], which is chip-free and
//! host-tested. What is genuinely a C6 fact is here and is only two things:
//! reading and clearing `USB_DEVICE.int_raw.sof`, and reading the device
//! clock. The S3 keeps the same pair for the same reason.

use fw_esp32_common::serial::link_counters::NEVER;
use fw_esp32_common::serial::usb_connection::UsbLinkState;

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

    /// Sample the SOF raw interrupt bit and update internal state.
    /// Call once per io_task loop iteration (~2ms).
    pub fn poll(&mut self) {
        let regs = esp_hal::peripherals::USB_DEVICE::regs();
        let sof_received = regs.int_raw().read().sof().bit_is_set();
        regs.int_clr().write(|w| w.sof().clear_bit_by_one());
        self.link.poll_with(sof_received);
    }

    /// A serial write timed out or failed: evidence nobody is draining.
    pub fn note_write_timeout(&mut self) {
        self.link.note_write_timeout(now_ms());
    }

    /// A serial write completed, or bytes arrived from the host: the host
    /// application is provably alive and draining.
    pub fn note_host_active(&mut self) {
        self.link.note_host_active(now_ms());
    }

    /// Should a probe write be attempted? True while enumerated but latched
    /// not-draining — the probe is the self-healing path for hosts that
    /// reopen the port without ever sending bytes (e.g. a passive monitor).
    pub fn needs_probe(&self) -> bool {
        self.link.needs_probe()
    }

    /// Attempt protocol writes only when the cable is enumerated AND the
    /// host application is draining the port.
    pub fn is_connected(&self) -> bool {
        self.link.is_connected()
    }
}

/// Milliseconds since boot on the device's own clock, saturating one short of
/// [`NEVER`].
///
/// `u32::MAX` is the "never happened" sentinel in the link counters, so an
/// uptime of 49.7 days must not accidentally spell it. Clamping is the honest
/// failure here: a stamp that far out is already useless as a latency, and
/// the alternative — wrapping — would read as a fresh transition.
fn now_ms() -> u32 {
    let ms = embassy_time::Instant::now().as_millis();
    ms.min(u64::from(NEVER) - 1) as u32
}
