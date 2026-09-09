//! The USB-Serial-JTAG link's connection state machine, without the chip.
//!
//! Two independent signals decide whether protocol writes should be
//! attempted:
//!
//! 1. **Cable/enumeration** — SOF (Start of Frame) packets. USB full-speed
//!    hosts send SOF every 1 ms; if they stop, the cable is unplugged or the
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
//!
//! Reading the SOF bit is a chip fact and stays in each chip's
//! `board::<chip>::usb_connection`; deciding what it *means* is not, and
//! lives here — where it can be driven by a host test rather than only by a
//! board. Both native-USB firmwares (C6, S3) held byte-identical copies of
//! this logic before M6 P1b, and adding the timestamps to both copies is
//! exactly the duplication that would have drifted.
//!
//! Every transition also stamps [`crate::serial::link_counters`], which is
//! the part that makes the state machine *observable*: both `log::info!`
//! lines below are written to the outgoing queue and then dropped by the
//! latch they report, because that queue is gated on `is_connected()`. On a
//! USB link they are self-erasing. The stamps are not — they ride the next
//! heartbeat after the host comes back, on the device's own clock, and they
//! are readable from outside a running image as symbols (the emulator's
//! `--probe`).

/// Missed-poll threshold before declaring disconnected.
/// io_task polls every ~2 ms, so 3 misses ≈ 6 ms without SOF — enough to
/// avoid false disconnects from tick jitter while still detecting quickly.
pub const DISCONNECT_THRESHOLD: u8 = 3;

/// Consecutive write timeouts before latching "host not draining".
/// One timeout can be a hiccup; two in a row (each a full write timeout)
/// means nobody is reading.
pub const NOT_DRAINING_THRESHOLD: u8 = 2;

/// The chip-free half of a USB-Serial-JTAG connection monitor.
///
/// The caller supplies the two chip facts: whether a SOF arrived since the
/// last poll, and what the device clock reads. Nothing here touches a
/// register or a timer.
pub struct UsbLinkState {
    no_sof_count: u8,
    write_timeouts: u8,
    host_draining: bool,
    /// `true` when the link is not really USB at all and SOF must not gate
    /// writes — the `spike_uart0_link` build, where the host link is UART0
    /// and no cable exists to detect.
    always_enumerated: bool,
}

impl UsbLinkState {
    pub const fn new(always_enumerated: bool) -> Self {
        Self {
            no_sof_count: 0,
            write_timeouts: 0,
            host_draining: true,
            always_enumerated,
        }
    }

    /// Fold one poll's SOF observation into the state.
    ///
    /// `sof_received` is the chip's `USB_DEVICE.int_raw.sof` bit, read and
    /// cleared by the caller.
    pub fn poll_with(&mut self, sof_received: bool) {
        if sof_received {
            self.no_sof_count = 0;
        } else {
            self.no_sof_count = self.no_sof_count.saturating_add(1);
            if !self.is_enumerated() {
                // Physical disconnect resets the draining latch: the next
                // enumeration starts from a clean slate. Deliberately NOT a
                // "draining again" stamp — nobody drained anything; the
                // question simply stopped being asked.
                self.write_timeouts = 0;
                self.host_draining = true;
            }
        }
    }

    /// A serial write timed out or failed: evidence nobody is draining.
    /// `now_ms` is the device clock, milliseconds since boot.
    pub fn note_write_timeout(&mut self, now_ms: u32) {
        self.write_timeouts = self.write_timeouts.saturating_add(1);
        if self.write_timeouts >= NOT_DRAINING_THRESHOLD && self.host_draining {
            self.host_draining = false;
            crate::serial::link_counters::note_host_not_draining(now_ms);
            log::info!("[io_task] host not draining; dropping protocol writes");
        }
    }

    /// A serial write completed, or bytes arrived from the host: the host
    /// application is provably alive and draining.
    pub fn note_host_active(&mut self, now_ms: u32) {
        self.write_timeouts = 0;
        if !self.host_draining {
            self.host_draining = true;
            crate::serial::link_counters::note_host_draining_again(now_ms);
            log::info!("[io_task] host draining again; resuming protocol writes");
        }
    }

    /// Should a probe write be attempted? True while enumerated but latched
    /// not-draining — the probe is the self-healing path for hosts that
    /// reopen the port without ever sending bytes (e.g. a passive monitor).
    pub fn needs_probe(&self) -> bool {
        self.is_enumerated() && !self.host_draining
    }

    pub fn is_enumerated(&self) -> bool {
        self.always_enumerated || self.no_sof_count < DISCONNECT_THRESHOLD
    }

    /// Attempt protocol writes only when the cable is enumerated AND the
    /// host application is draining the port.
    pub fn is_connected(&self) -> bool {
        self.is_enumerated() && self.host_draining
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::link_counters;

    /// The whole negative control, on the host — and deliberately **one**
    /// test function.
    ///
    /// The stamps ride module-global counters (the ones the heartbeat reads
    /// and the ones the emulator's `--probe` resolves by symbol), so two test
    /// functions driving two `UsbLinkState`s would race for them under the
    /// harness's threads. One function, three phases, in order.
    #[test]
    fn the_monitor_latches_resumes_and_stamps_each_transition() {
        assert_eq!(
            link_counters::host_not_draining_ms(),
            None,
            "nothing may have latched before this test runs"
        );

        // --- phase 1: attached, nobody reading -----------------------------
        let mut link = UsbLinkState::new(false);
        assert!(link.is_connected(), "a fresh link is optimistic");

        // Attached: SOF every poll, so enumeration never lapses.
        for _ in 0..10 {
            link.poll_with(true);
        }
        assert!(link.is_enumerated());
        assert!(link.is_connected());
        assert!(!link.needs_probe(), "nothing to probe while it looks fine");

        // One timeout is a hiccup, not a verdict.
        link.note_write_timeout(1_100);
        assert!(link.is_connected(), "one timeout must not latch");
        assert_eq!(
            link_counters::not_draining_count(),
            0,
            "one timeout is not a silence"
        );

        // Two in a row is.
        link.note_write_timeout(1_350);
        assert!(!link.is_connected(), "latched");
        assert!(link.needs_probe(), "and the probe path opens");
        assert_eq!(link_counters::host_not_draining_ms(), Some(1_350));
        assert_eq!(
            link_counters::host_draining_again_ms(),
            None,
            "not recovered yet — and `None` means exactly that on the wire"
        );
        assert_eq!(link_counters::not_draining_count(), 1);

        // Further timeouts while latched must not re-latch: the count is
        // "how many silences", not "how many failed writes".
        link.note_write_timeout(1_600);
        link.note_write_timeout(1_850);
        assert!(!link.is_connected());
        assert_eq!(link_counters::not_draining_count(), 1);
        assert_eq!(link_counters::host_not_draining_ms(), Some(1_350));

        // The host opens the port; the next probe write completes.
        link.note_host_active(8_120);
        assert!(link.is_connected(), "resumed");
        assert!(!link.needs_probe());
        assert_eq!(link_counters::host_not_draining_ms(), Some(1_350));
        assert_eq!(link_counters::host_draining_again_ms(), Some(8_120));
        assert_eq!(link_counters::not_draining_count(), 1);
        assert!(
            link_counters::host_not_draining_ms() < link_counters::host_draining_again_ms(),
            "the order is the claim: silence, then recovery"
        );

        // --- phase 2: the cable, not the application -----------------------
        // A physical disconnect clears the latch without claiming a recovery:
        // re-enumeration starts optimistic, and the next two timeouts are a
        // new silence rather than a continuation of the old one.
        link.note_write_timeout(9_000);
        link.note_write_timeout(9_250);
        assert!(!link.is_connected());
        assert_eq!(link_counters::not_draining_count(), 2);
        assert_eq!(link_counters::host_not_draining_ms(), Some(9_250));
        assert_eq!(
            link_counters::host_draining_again_ms(),
            Some(8_120),
            "the earlier recovery stamp stands until it is superseded — the \
             pair is read together, never as a duration on its own"
        );

        for _ in 0..DISCONNECT_THRESHOLD {
            link.poll_with(false);
        }
        assert!(!link.is_enumerated(), "cable gone");
        assert!(!link.needs_probe(), "no cable, no probe");
        assert_eq!(
            link_counters::host_draining_again_ms(),
            Some(8_120),
            "losing the cable is not a recovery: nobody drained anything, the \
             question simply stopped being asked"
        );

        link.poll_with(true);
        assert!(link.is_connected(), "re-attach starts optimistic");

        // --- phase 3: the link that is not USB -----------------------------
        // The `spike_uart0_link` build: the host link is UART0, there is no
        // cable, and SOF must never gate a write.
        let mut uart = UsbLinkState::new(true);
        for _ in 0..100 {
            uart.poll_with(false);
        }
        assert!(uart.is_enumerated());
        assert!(uart.is_connected());
    }
}
