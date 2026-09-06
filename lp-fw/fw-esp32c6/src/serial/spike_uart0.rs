//! `spike_uart0_link` — the host link over UART0 instead of USB-Serial-JTAG.
//!
//! Test scaffolding for the 2026-09-06 esp-emu spike, never shipped. Under
//! Espressif's binary emulator the USB-Serial-JTAG block never raises SOF
//! (`usb_connection.rs` therefore reports "cable unplugged" forever) and
//! bytes written to its FIFO vanish — the emulator reports
//! `SERIAL_IN_EP_DATA_FREE` set and delivers nothing. Its only host-visible
//! serial is UART0, bridged to stdout or `--uart-tcp`. This module gives the
//! firmware a UART0 path so the wire protocol and the harnesses can be
//! exercised there:
//!
//! - [`link`] hands `io_task` an async `Uart` on UART0 (GPIO16 TX / GPIO17
//!   RX, the chip's default U0 pads) in place of the USB halves. The rest of
//!   `io_task` is already generic over `embedded_io_async::{Read, Write}`.
//! - [`rom_tx_bytes`] is the one-line tee `Esp32UsbSerialIo::write` uses so
//!   the harnesses' output reaches UART0 too. It goes through the mask ROM's
//!   `uart_tx_one_char` at the address esp-println's own `uart` printer
//!   uses for this chip, so it needs neither a driver nor a linker symbol.
//!
//! The peripherals are `steal()`ed rather than threaded through
//! `init_board`'s tuple: this feature must not touch the shipped image's
//! ownership chain. Nothing else in this crate claims UART0 or GPIO16/17.

use esp_hal::Async;
use esp_hal::uart::{Config, Uart};

/// Same rate as the classic's UART0 host link (`fw-esp32v3`); irrelevant to
/// the emulator's TCP bridge, which carries bytes, not bit timing.
pub const BAUD: u32 = 921_600;

/// Mask-ROM `uart_tx_one_char` on the ESP32-C6 (`esp32c6.rom.ld`), the same
/// constant esp-println's `uart` printer transmutes for this chip.
#[allow(dead_code, reason = "used by the harness tee, like `rom_tx_bytes`")]
const ROM_UART_TX_ONE_CHAR: usize = 0x4000_0058;

/// Build the async UART0 link for `io_task`.
pub fn link() -> Uart<'static, Async> {
    // SAFETY: this feature is the only owner of UART0 and of GPIO16/17 in the
    // image — the app never constructs a UART, and the board manifest's
    // WS281x outputs are GPIO18/20 (D10/D9). Called once, from io_task.
    let (uart0, tx, rx) = unsafe {
        (
            esp_hal::peripherals::UART0::steal(),
            esp_hal::peripherals::GPIO16::steal(),
            esp_hal::peripherals::GPIO17::steal(),
        )
    };
    Uart::new(uart0, Config::default().with_baudrate(BAUD))
        .expect("spike_uart0_link: UART0 config rejected")
        .with_tx(tx)
        .with_rx(rx)
        .into_async()
}

/// Blocking byte-at-a-time write through the ROM, for the harness tee.
#[allow(
    dead_code,
    reason = "called from `Esp32UsbSerialIo::write`, which only the harness entry points construct"
)]
pub fn rom_tx_bytes(bytes: &[u8]) {
    // SAFETY: the ROM entry is a fixed, always-mapped function on this chip
    // with the C ABI `int uart_tx_one_char(uint8_t)`; it polls UART0's TX
    // FIFO for space and returns. It only touches UART0 registers.
    let tx_one_char: unsafe extern "C" fn(u8) -> i32 =
        unsafe { core::mem::transmute(ROM_UART_TX_ONE_CHAR) };
    for &b in bytes {
        unsafe { tx_one_char(b) };
    }
}
