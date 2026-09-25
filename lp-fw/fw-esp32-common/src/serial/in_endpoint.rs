//! The io_task's TX half, made safe to share the USB-Serial-JTAG IN endpoint.
//!
//! The USB-Serial-JTAG block has **one** 64-byte send buffer. Once it is
//! flushed (`wr_done`, or the 64th byte) firmware cannot write it until the
//! host has read all of it, and only then does `SERIAL_IN_EMPTY_INT` fire
//! (ESP32-C3 TRM v1.3 §30.3.2, p. 767 — the same IP block the C6 and S3
//! carry; both PACs' `SERIAL_IN_EP_DATA_FREE` doc says the bit reads 0 from
//! `wr_done` until the host has read the data). The model and its
//! register-level tests: `lp-emu/esp/lp-emu-esp-common/src/ip/usb_sj.rs`.
//!
//! esp-hal 1.1.1's async `write_async` trusts that nobody else writes the
//! endpoint: it pushes a chunk without reading `serial_in_ep_data_free`, sets
//! `wr_done`, and awaits a future whose `new` only **enables**
//! `serial_in_empty` — it never clears the raw bit first. esp-println (every
//! `[INIT]` line, the heartbeat triple, the panic path) is a second, polled
//! writer on the same endpoint and never clears that raw bit either. So two
//! things go wrong:
//!
//! 1. a chunk written while another packet is still pending lands in a buffer
//!    the block will not take (on the S3 a stop-all's reply was lost this
//!    way);
//! 2. a chunk's write future wakes on a raw bit an earlier drain left behind,
//!    a few hundred cycles after `wr_done`, and the *next* chunk is written
//!    into the still-pending packet (one 64-byte packet of the S3's boot
//!    `hello` was lost this way until the timing moved).
//!
//! [`InEndpoint`] fixes both before every write, without touching esp-hal:
//! wait until the buffer is free, then clear the now-stale `serial_in_empty`,
//! so the only thing that can raise it is the drain of the packet this write
//! commits. It wraps the io_task's **whole** TX half, so every byte the task
//! writes — a JSON `M!` line, a packed `\n 0x00 'L' COBS 0x00` frame
//! ([`super::server_msg`]), each `ChunkedWriter` chunk of either, the log
//! lines and the not-draining probe — passes the gate.
//!
//! The two register touches are chip facts this crate may not hold (no
//! esp-hal here — see Cargo.toml's seam rules), so they arrive as
//! [`InEndpointRegs`], implemented beside each chip's `usb_connection`. The
//! classic ESP32 (`fw-esp32v3`) has no USB-Serial-JTAG block: it never names
//! this type, and a generic nobody instantiates costs its image nothing.
//!
//! See `docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`
//! (the mechanism, PR #805) and
//! `docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md`
//! (the C6's silicon symptom this gate is the candidate fix for).

use core::marker::PhantomData;

use embedded_io_async::{ErrorType, Write};

/// The IN endpoint's packet size: the one send buffer's 64 bytes.
const PACKET_BYTES: usize = 64;

/// The USB-Serial-JTAG register touches the gate needs — a chip fact.
pub trait InEndpointRegs {
    /// `EP1_CONF.SERIAL_IN_EP_DATA_FREE`: the send buffer can be written.
    fn in_ep_free() -> bool;
    /// Clear the raw `SERIAL_IN_EMPTY` interrupt (`INT_CLR`).
    fn clear_serial_in_empty();
}

/// A TX half behind the free-then-clear gate (module docs).
///
/// `W::flush` must wait for the send buffer to drain (esp-hal's
/// `UsbSerialJtagTx<Async>` does: it arms `serial_in_empty` while
/// `serial_in_ep_data_free` is clear).
pub struct InEndpoint<W, R> {
    tx: W,
    regs: PhantomData<R>,
}

impl<W: Write, R: InEndpointRegs> InEndpoint<W, R> {
    pub fn new(tx: W) -> Self {
        Self {
            tx,
            regs: PhantomData,
        }
    }

    /// Wait until the send buffer is free, then clear the stale raw bit.
    ///
    /// The clear-then-recheck order is what makes the wait race-free: if the
    /// buffer drains after the first check, either the recheck sees it free,
    /// or the drain raises the bit *after* the clear and the inner `flush`
    /// (which arms the interrupt only while the buffer is still not free)
    /// wakes on it.
    async fn ready(&mut self) -> Result<(), W::Error> {
        if !R::in_ep_free() {
            R::clear_serial_in_empty();
            if !R::in_ep_free() {
                self.tx.flush().await?;
            }
        }
        // Free now, so a set raw bit can only be a drain that already
        // happened: stale. Clearing it leaves this write's own drain as the
        // one thing that completes esp-hal's write future.
        R::clear_serial_in_empty();
        Ok(())
    }
}

impl<W: Write, R> ErrorType for InEndpoint<W, R> {
    type Error = W::Error;
}

impl<W: Write, R: InEndpointRegs> Write for InEndpoint<W, R> {
    /// Gate, then hand the inner writer **at most one packet**.
    ///
    /// esp-hal's `write_async` splits a longer buffer into 64-byte packets
    /// itself and writes each after awaiting the previous one's drain — with
    /// no free check, so a polled esp-println write that lands between two of
    /// its packets would be overwritten by the next. Capping each `write` at
    /// one packet puts the gate in front of every packet, and `write_all`
    /// (which `ChunkedWriter` uses) loops over the rest.
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.ready().await?;
        let packet = buf.len().min(PACKET_BYTES);
        self.tx.write(&buf[..packet]).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.tx.flush().await
    }
}
