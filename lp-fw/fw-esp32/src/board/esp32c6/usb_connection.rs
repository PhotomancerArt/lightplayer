//! USB-Serial-JTAG connection monitor for ESP32-C6.
//!
//! Detects whether a USB host is connected by monitoring SOF (Start of Frame)
//! packets. USB full-speed hosts send SOF every 1ms. If we stop seeing them,
//! the host is disconnected.
//!
//! Based on the same approach used by ESP-IDF's
//! `usb_serial_jtag_connection_monitor.c`.

/// Missed-poll threshold before declaring disconnected.
/// io_task polls every ~2ms, so 3 misses ≈ 6ms without SOF — enough to
/// avoid false disconnects from tick jitter while still detecting quickly.
const DISCONNECT_THRESHOLD: u8 = 3;

pub struct UsbConnectionMonitor {
    no_sof_count: u8,
}

impl UsbConnectionMonitor {
    pub fn new() -> Self {
        Self { no_sof_count: 0 }
    }

    /// Sample the SOF raw interrupt bit and update internal state.
    /// Call once per io_task loop iteration (~2ms).
    pub fn poll(&mut self) {
        let regs = esp_hal::peripherals::USB_DEVICE::regs();
        let sof_received = regs.int_raw().read().sof().bit_is_set();
        regs.int_clr().write(|w| w.sof().clear_bit_by_one());

        if sof_received {
            self.no_sof_count = 0;
        } else {
            self.no_sof_count = self.no_sof_count.saturating_add(1);
        }
    }

    pub fn is_connected(&self) -> bool {
        self.no_sof_count < DISCONNECT_THRESHOLD
    }
}
