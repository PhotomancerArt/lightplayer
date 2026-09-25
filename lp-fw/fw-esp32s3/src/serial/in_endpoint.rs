//! The io_task's TX half, made safe to share the IN endpoint with esp-println.
//!
//! The USB-Serial-JTAG block has **one** 64-byte send buffer. Once it is
//! flushed (`wr_done`, or the 64th byte) firmware cannot write it until the
//! host has read all of it, and only then does `SERIAL_IN_EMPTY_INT` fire
//! (ESP32-C3 TRM v1.3 §30.3.2, p. 767 — the same IP block; the PAC's
//! `SERIAL_IN_EP_DATA_FREE` doc says the bit reads 0 from `wr_done` until the
//! host has read the data). The model and its register-level tests:
//! `lp-emu/esp/lp-emu-esp-common/src/ip/usb_sj.rs`.
//!
//! esp-hal 1.1.1's async `write_async` trusts that nobody else writes the
//! endpoint: it pushes a chunk without reading `serial_in_ep_data_free`, sets
//! `wr_done`, and awaits a future whose `new` only **enables**
//! `serial_in_empty` — it never clears the raw bit first. esp-println (every
//! `[INIT]` line and the heartbeat triple) is a second, polled writer on the
//! same endpoint and never clears that raw bit either. So two things go wrong:
//!
//! 1. a chunk written while esp-println's last packet is still pending lands in
//!    a buffer the block will not take (a stop-all's reply was lost this way);
//! 2. a chunk's write future wakes on the raw bit esp-println's drain left
//!    behind, a few hundred cycles after `wr_done`, and the *next* chunk is
//!    written into the still-pending packet (one 64-byte packet of the boot
//!    `hello` was lost this way until the timing moved).
//!
//! [`InEndpoint`] fixes both before every write, without touching esp-hal:
//! wait until the buffer is free, then clear the now-stale `serial_in_empty`,
//! so the only thing that can raise it is the drain of the packet this write
//! commits. See
//! `docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`.

use embedded_io_async::{ErrorType, Write};
use esp_hal::Async;
use esp_hal::peripherals::USB_DEVICE;
use esp_hal::usb_serial_jtag::UsbSerialJtagTx;

/// esp-hal's async TX half behind the free-then-clear gate (module docs).
pub struct InEndpoint<'d> {
    tx: UsbSerialJtagTx<'d, Async>,
}

impl<'d> InEndpoint<'d> {
    pub fn new(tx: UsbSerialJtagTx<'d, Async>) -> Self {
        Self { tx }
    }

    /// Wait until the send buffer is free, then clear the stale raw bit.
    ///
    /// The clear-then-recheck order is what makes the wait race-free: if the
    /// buffer drains after the first check, either the recheck sees it free,
    /// or the drain raises the bit *after* the clear and esp-hal's `flush`
    /// (which arms the interrupt only while the buffer is still not free)
    /// wakes on it.
    async fn ready(&mut self) -> Result<(), <Self as ErrorType>::Error> {
        if !in_ep_free() {
            clear_serial_in_empty();
            if !in_ep_free() {
                Write::flush(&mut self.tx).await?;
            }
        }
        // Free now, so a set raw bit can only be a drain that already
        // happened: stale. Clearing it leaves this write's own drain as the
        // one thing that completes esp-hal's write future.
        clear_serial_in_empty();
        Ok(())
    }
}

impl ErrorType for InEndpoint<'_> {
    type Error = <UsbSerialJtagTx<'static, Async> as ErrorType>::Error;
}

impl Write for InEndpoint<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.ready().await?;
        Write::write(&mut self.tx, buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Write::flush(&mut self.tx).await
    }
}

fn in_ep_free() -> bool {
    USB_DEVICE::regs()
        .ep1_conf()
        .read()
        .serial_in_ep_data_free()
        .bit_is_set()
}

fn clear_serial_in_empty() {
    USB_DEVICE::regs()
        .int_clr()
        .write(|w| w.serial_in_empty().clear_bit_by_one());
}
