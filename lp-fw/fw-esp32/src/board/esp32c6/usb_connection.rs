//! USB-Serial-JTAG connection monitor for ESP32-C6.
//!
//! Two independent signals decide whether protocol writes should be
//! attempted:
//!
//! 1. **Cable/enumeration** — SOF (Start of Frame) packets. USB full-speed
//!    hosts send SOF every 1ms; if they stop, the cable is unplugged or the
//!    device de-enumerated. (Same approach as ESP-IDF's
//!    `usb_serial_jtag_connection_monitor.c`.)
//! 2. **Host application draining** — SOF keeps arriving as long as the
//!    cable is plugged, even when no application has the port open. In that
//!    state the TX FIFO fills and every write times out; unchecked, those
//!    timeouts stall the io task (frame stutter) and once starved the
//!    recovery watchdog reboots the device. Consecutive write timeouts
//!    therefore latch "not draining" and writes are dropped fast until the
//!    host proves itself again (incoming bytes, or a periodic probe write
//!    succeeding).

/// Missed-poll threshold before declaring disconnected.
/// io_task polls every ~2ms, so 3 misses ≈ 6ms without SOF — enough to
/// avoid false disconnects from tick jitter while still detecting quickly.
const DISCONNECT_THRESHOLD: u8 = 3;

/// Consecutive write timeouts before latching "host not draining".
/// One timeout can be a hiccup; two in a row (each a full write timeout)
/// means nobody is reading.
const NOT_DRAINING_THRESHOLD: u8 = 2;

pub struct UsbConnectionMonitor {
    no_sof_count: u8,
    write_timeouts: u8,
    host_draining: bool,
}

impl UsbConnectionMonitor {
    pub fn new() -> Self {
        Self {
            no_sof_count: 0,
            write_timeouts: 0,
            host_draining: true,
        }
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
            if !self.is_enumerated() {
                // Physical disconnect resets the draining latch: the next
                // enumeration starts from a clean slate.
                self.write_timeouts = 0;
                self.host_draining = true;
            }
        }
    }

    /// A serial write timed out or failed: evidence nobody is draining.
    pub fn note_write_timeout(&mut self) {
        self.write_timeouts = self.write_timeouts.saturating_add(1);
        if self.write_timeouts >= NOT_DRAINING_THRESHOLD && self.host_draining {
            self.host_draining = false;
            log::info!("[io_task] host not draining; dropping protocol writes");
        }
    }

    /// A serial write completed, or bytes arrived from the host: the host
    /// application is provably alive and draining.
    pub fn note_host_active(&mut self) {
        self.write_timeouts = 0;
        if !self.host_draining {
            self.host_draining = true;
            log::info!("[io_task] host draining again; resuming protocol writes");
        }
    }

    /// Should a probe write be attempted? True while enumerated but latched
    /// not-draining — the probe is the self-healing path for hosts that
    /// reopen the port without ever sending bytes (e.g. a passive monitor).
    pub fn needs_probe(&self) -> bool {
        self.is_enumerated() && !self.host_draining
    }

    fn is_enumerated(&self) -> bool {
        self.no_sof_count < DISCONNECT_THRESHOLD
    }

    /// Attempt protocol writes only when the cable is enumerated AND the
    /// host application is draining the port.
    pub fn is_connected(&self) -> bool {
        self.is_enumerated() && self.host_draining
    }
}
