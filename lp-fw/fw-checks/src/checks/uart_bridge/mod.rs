//! The `uart-bridge` payload: turn a spare board into the lab's USB-to-UART tap.
//!
//! There is no USB-to-UART adapter on this bench, and the one measurement the
//! emulator can never produce — a firmware declaring its USB host undrained —
//! is logged over the very link it declares undrained (`g3-desk-batch.md`,
//! step 4). The way out is a **second board**: its USB-Serial-JTAG link to the
//! Mac on one side, UART0 on the other, pumping bytes both ways and saying
//! nothing of its own. Then the device under test logs into a console that is
//! not the link under test.
//!
//! ## The contract
//!
//! Every byte that arrives on USB-Serial-JTAG leaves on UART0 TX; every byte
//! that arrives on UART0 RX leaves on USB-Serial-JTAG. No framing, no
//! protocol, no escaping — a tap that edited the stream would not be a tap.
//!
//! Two lines are printed at boot, on the USB side only, and then the bridge is
//! silent on its own behalf forever:
//!
//! ```text
//! [fw-checks-header] {"schema":1,"payload":"uart-bridge", …}
//! UART-BRIDGE READY baud=115200 tx=gpio16 rx=gpio17 prev_drop_to_uart=0 prev_drop_to_usb=0
//! ```
//!
//! After those, a host reading the bridge's port sees the other board's bytes
//! and nothing else. That is the whole value of the instrument, and it is why
//! the firmware half installs no `log` sink at all: with no logger registered,
//! every `log::` call in esp-hal and in this crate is a no-op, so a stray
//! `debug!` cannot appear in the middle of somebody's boot capture.
//!
//! ## Why the drop counts are a boot-time report
//!
//! A bridge whose sink stalls has to drop bytes (see [`ring`]), and a bridge
//! that *announced* the drop would be writing into the stream it is supposed
//! to carry — corrupting the capture at exactly the moment the capture got
//! interesting. So the counts are never printed mid-stream. They accumulate,
//! and the **next boot's** ready line reports them; the firmware half keeps
//! them in RTC fast memory so they survive the reset that produces that line.
//! A power cycle clears them, which is honest: a bridge that has just been
//! plugged in has dropped nothing.
//!
//! `prev_drop_to_uart=0 prev_drop_to_usb=0` therefore means "the run before
//! this reset was clean", and a non-zero count means the capture that came out
//! of that run has a hole in it and must not be trusted.

pub mod ring;

use core::fmt;

pub use ring::ByteRing;

/// The readiness line's prefix — the payload's sentinel. It serves forever;
/// there is no done marker.
pub const BRIDGE_READY_PREFIX: &str = "UART-BRIDGE READY ";

/// The ROM console rate, and this payload's default.
///
/// The device under test's earliest bytes come out of the mask ROM's
/// `uart_tx_one_char`, which runs at the ROM's own rate and is not affected by
/// anything a driver configures later. A bridge at any other rate cannot read
/// a boot banner. `spike_uart0_link`'s 921,600 is the rate to match when the
/// far side is a *driver* rather than the ROM; the firmware half selects it
/// with a cargo feature.
pub const ROM_CONSOLE_BAUD: u32 = 115_200;

/// The rate `fw-esp32c6`'s `spike_uart0_link` driver runs UART0 at, and the
/// rate the bridge must match when the far side is that driver rather than the
/// ROM. Named here so the two never drift apart in someone's head at a desk.
pub const SPIKE_LINK_BAUD: u32 = 921_600;

/// The ESP32-C6's default U0TXD pad — `D6` on the XIAO silkscreen.
pub const UART0_TX_GPIO: u8 = 16;

/// The ESP32-C6's default U0RXD pad — `D7` on the XIAO silkscreen.
pub const UART0_RX_GPIO: u8 = 17;

/// The readiness line, and the previous run's losses with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadyLine {
    pub baud: u32,
    pub tx_gpio: u8,
    pub rx_gpio: u8,
    /// Bytes lost on the way from USB-Serial-JTAG to UART0 TX, last run.
    pub prev_drop_to_uart: u32,
    /// Bytes lost on the way from UART0 RX to USB-Serial-JTAG, last run.
    pub prev_drop_to_usb: u32,
}

impl fmt::Display for ReadyLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{BRIDGE_READY_PREFIX}baud={} tx=gpio{} rx=gpio{} \
             prev_drop_to_uart={} prev_drop_to_usb={}",
            self.baud, self.tx_gpio, self.rx_gpio, self.prev_drop_to_uart, self.prev_drop_to_usb
        )
    }
}

/// What one [`pump`] step moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PumpStep {
    /// Bytes taken from the source into the queue.
    pub accepted: usize,
    /// Bytes the source produced that did not fit, and are gone.
    pub dropped: usize,
    /// Bytes handed to the sink.
    pub emitted: usize,
}

/// One step of one direction: everything the source produced goes into the
/// queue as far as it fits, then the oldest of the queue comes back out into
/// `out` for the sink.
///
/// Splitting it this way — rather than copying source straight to sink — is
/// what lets a stalled sink be *counted* instead of silently back-pressuring
/// the source's hardware FIFO into an overrun nobody sees. `incoming` may be
/// empty (a step that only drains), and `out` may be shorter than the queue (a
/// sink that takes fixed-size chunks); neither reorders anything.
///
/// The emitted bytes are removed from the queue before the caller writes them.
/// That is deliberate and it is why the firmware half only ever hands them to
/// a sink whose write commits the bytes to a hardware FIFO before it yields:
/// "emitted" means delivered or queued in silicon, never "still ours".
pub fn pump<const N: usize>(queue: &mut ByteRing<N>, incoming: &[u8], out: &mut [u8]) -> PumpStep {
    let accepted = queue.push(incoming);
    let emitted = queue.pop_into(out);
    PumpStep {
        accepted,
        dropped: incoming.len() - accepted,
        emitted,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{format, vec::Vec};

    use super::*;

    /// Run a whole stream through one direction, `chunk` bytes of sink per
    /// step, and return what the sink saw.
    fn drive<const N: usize>(stream: &[&[u8]], chunk: usize) -> (Vec<u8>, usize) {
        let mut queue = ByteRing::<N>::new();
        let mut out = std::vec![0u8; chunk];
        let mut seen = Vec::new();
        let mut dropped = 0;
        for piece in stream {
            let step = pump(&mut queue, piece, &mut out);
            seen.extend_from_slice(&out[..step.emitted]);
            dropped += step.dropped;
        }
        // Drain whatever is left, as the firmware's idle steps do.
        loop {
            let step = pump(&mut queue, &[], &mut out);
            if step.emitted == 0 {
                break;
            }
            seen.extend_from_slice(&out[..step.emitted]);
        }
        (seen, dropped)
    }

    #[test]
    fn a_stream_that_fits_arrives_byte_for_byte() {
        let (seen, dropped) = drive::<64>(&[b"hello ", b"world", b"!\n"], 64);
        assert_eq!(seen, b"hello world!\n".to_vec());
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_sink_that_takes_small_chunks_still_preserves_order() {
        // 8 bytes in per step, 3 out: the queue grows, and the order must not.
        let stream: Vec<&[u8]> = std::vec![b"abcdefgh"; 6];
        let (seen, dropped) = drive::<64>(&stream, 3);
        assert_eq!(seen, b"abcdefgh".repeat(6));
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_stalled_sink_loses_the_tail_and_says_how_much() {
        // Nothing comes out (chunk 0), so the 4-byte queue fills and every
        // further byte is counted.
        let mut queue = ByteRing::<4>::new();
        let mut out = [0u8; 0];
        let first = pump(&mut queue, b"abcdef", &mut out);
        assert_eq!(first.accepted, 4);
        assert_eq!(first.dropped, 2);
        assert_eq!(first.emitted, 0);

        let second = pump(&mut queue, b"ghi", &mut out);
        assert_eq!(second.accepted, 0);
        assert_eq!(second.dropped, 3);
        assert_eq!(queue.dropped(), 5, "the queue carries the running total");

        // When the sink comes back, what is left is the *earliest* bytes.
        let mut out = [0u8; 8];
        let third = pump(&mut queue, &[], &mut out);
        assert_eq!(&out[..third.emitted], b"abcd");
    }

    #[test]
    fn an_empty_step_is_a_drain() {
        let mut queue = ByteRing::<8>::new();
        let mut nowhere = [0u8; 0];
        pump(&mut queue, b"abc", &mut nowhere);
        let mut out = [0u8; 8];
        let step = pump(&mut queue, &[], &mut out);
        assert_eq!(
            step,
            PumpStep {
                accepted: 0,
                dropped: 0,
                emitted: 3
            }
        );
        assert_eq!(&out[..3], b"abc");
    }

    #[test]
    fn the_ready_line_renders_the_contract() {
        let line = ReadyLine {
            baud: ROM_CONSOLE_BAUD,
            tx_gpio: UART0_TX_GPIO,
            rx_gpio: UART0_RX_GPIO,
            prev_drop_to_uart: 0,
            prev_drop_to_usb: 0,
        };
        assert_eq!(
            format!("{line}"),
            "UART-BRIDGE READY baud=115200 tx=gpio16 rx=gpio17 \
             prev_drop_to_uart=0 prev_drop_to_usb=0"
        );
        assert!(format!("{line}").starts_with(BRIDGE_READY_PREFIX));
    }

    #[test]
    fn the_ready_line_reports_a_dirty_previous_run() {
        let line = ReadyLine {
            baud: 921_600,
            tx_gpio: UART0_TX_GPIO,
            rx_gpio: UART0_RX_GPIO,
            prev_drop_to_uart: 7,
            prev_drop_to_usb: 4_294_967_295,
        };
        assert_eq!(
            format!("{line}"),
            "UART-BRIDGE READY baud=921600 tx=gpio16 rx=gpio17 \
             prev_drop_to_uart=7 prev_drop_to_usb=4294967295"
        );
    }
}
