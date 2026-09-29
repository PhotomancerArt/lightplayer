//! Whether a USB host enumerates the board, without the chip.
//!
//! USB full-speed hosts send a SOF (Start of Frame) packet every 1 ms; if they
//! stop, the cable is out or the device de-enumerated. (Same approach as
//! ESP-IDF's `usb_serial_jtag_connection_monitor.c`.) A charger or power bank
//! sends none, so it does not count as a host.
//!
//! Reading the SOF bit is a chip fact and stays in each chip's
//! `board::<chip>::usb_connection`; deciding what it *means* is not, and lives
//! here, where a host test can drive it. Two readers use the answer: the USB
//! link task (a frame is not written while nothing enumerates the board —
//! it would only time out) and the C6's power platform (switch-mode power-off
//! must not drop a board a computer is talking to).
//!
//! Until the USB link moved onto lp-link (plan `lp-link-usb-cutover`) this
//! also held a "host not draining" latch that dropped replies after two write
//! timeouts, and a link epoch that reset the packed encoding when the host
//! went away. lp-link's session does both jobs now (D4, D8): its stall
//! detection and its `Reset`, on both ends at once.

/// Missed polls before declaring the host gone. The link task samples SOF at
/// most every 2 ms and at least every 10 ms, and the bit is latched between
/// samples, so three samples without one is at least 6 ms without SOF —
/// enough to ride out tick jitter while still noticing an unplug quickly.
/// (Sampling faster than the 1 ms SOF period would count misses that are not
/// there; the task's rate limit is what makes this threshold mean time.)
pub const DISCONNECT_THRESHOLD: u8 = 3;

/// The chip-free half of a USB-Serial-JTAG cable monitor.
///
/// The caller supplies the chip fact: whether a SOF arrived since the last
/// poll. Nothing here touches a register or a timer.
pub struct UsbLinkState {
    no_sof_count: u8,
    /// `true` when the link is not really USB at all and SOF must not gate
    /// writes — the `spike_uart0_link` build, where the host link is UART0
    /// and no cable exists to detect.
    always_enumerated: bool,
}

impl UsbLinkState {
    pub const fn new(always_enumerated: bool) -> Self {
        Self {
            no_sof_count: 0,
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
        }
    }

    /// A USB host enumerates the board (or the link is not USB at all).
    pub fn is_enumerated(&self) -> bool {
        self.always_enumerated || self.no_sof_count < DISCONNECT_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cable_is_gone_after_three_polls_without_sof_and_back_with_one() {
        let mut link = UsbLinkState::new(false);
        assert!(link.is_enumerated(), "a fresh monitor is optimistic");
        for _ in 0..10 {
            link.poll_with(true);
        }
        assert!(link.is_enumerated());

        for _ in 0..DISCONNECT_THRESHOLD - 1 {
            link.poll_with(false);
        }
        assert!(link.is_enumerated(), "a missed SOF or two is jitter");
        link.poll_with(false);
        assert!(!link.is_enumerated(), "cable gone");
        for _ in 0..300 {
            link.poll_with(false);
        }
        assert!(!link.is_enumerated(), "and stays gone, however long");

        link.poll_with(true);
        assert!(link.is_enumerated(), "one SOF and it is back");
    }

    /// The `spike_uart0_link` build: the host link is UART0, there is no
    /// cable, and SOF must never gate a write.
    #[test]
    fn a_link_that_is_not_usb_is_always_enumerated() {
        let mut uart = UsbLinkState::new(true);
        for _ in 0..100 {
            uart.poll_with(false);
        }
        assert!(uart.is_enumerated());
    }
}
