//! The `uart-bridge` payload's device half: a XIAO C6 as the lab's USB-to-UART tap.
//!
//! Bytes in on USB-Serial-JTAG go out UART0 TX (GPIO16 / `D6`); bytes in on
//! UART0 RX (GPIO17 / `D7`) go out USB-Serial-JTAG. Two lines at boot, then
//! silence. The contract, the queue and the pump step live in
//! `fw_checks::checks::uart_bridge`, where they are `no_std`, alloc-free and
//! tested on the host; what stays here is the part that needs a chip.
//!
//! ## No logger, on purpose
//!
//! This harness never calls `logger::init`. With no `log` sink registered every
//! `log::` call in this crate and in esp-hal is a no-op, which is the only way
//! to guarantee that an `esp_hal` `debug!` cannot land in the middle of
//! somebody's boot capture. It is also why the two boot lines go out through
//! `esp_println` (whose USB-Serial-JTAG printer times out and drops rather than
//! spinning on an unread endpoint) instead of through `fw_checks::emit_header`,
//! which logs. The line they produce is byte-identical to the contracted one —
//! `PayloadHeader`'s `Display` is the same renderer `emit_header` uses.
//!
//! ## The two directions are not symmetric
//!
//! UART0 TX always drains: bytes leave at the line rate whether or not anything
//! is listening. So USB → UART needs no timeout, and a host that floods the
//! bridge is simply throttled — which is what a bridge should do.
//!
//! USB-Serial-JTAG TX does **not** drain unless a host is reading the port. So
//! UART → USB is bounded at every step: the endpoint is checked free before a
//! packet is committed to it, the commit itself is bounded, and while the host
//! is away the queue absorbs and counts instead of blocking. Blocking there
//! would back the bytes up into UART0's 128-byte hardware RX FIFO, which
//! overruns silently — the exact failure this payload exists to avoid.
//!
//! ## Drop accounting
//!
//! Counts live in RTC fast memory and survive a reset, so the ready line of the
//! **next** boot reports the run that just ended (a power cycle clears them,
//! which is honest: a freshly plugged-in bridge has dropped nothing). A count
//! of `4294967295` means UART0's hardware RX FIFO overran: the bridge knows it
//! lost bytes and does not know how many, and saying `u32::MAX` is the only
//! non-fiction available.

use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_hal::Async;
use esp_hal::uart::{Config, Uart, UartRx, UartTx};
use esp_hal::usb_serial_jtag::{UsbSerialJtag, UsbSerialJtagRx, UsbSerialJtagTx};
use fw_checks::checks::uart_bridge::{ByteRing, ReadyLine, UART0_RX_GPIO, UART0_TX_GPIO, pump};

use crate::board::esp32c6::init::{init_board, start_runtime};

const TARGET: &str = "esp32c6";

/// The line rate UART0 runs at.
///
/// The default is the mask ROM's, because the first bytes a device under test
/// ever emits come from `uart_tx_one_char` and no driver setting can move them.
/// `uart_bridge_fast` selects `spike_uart0_link`'s 921,600 instead, for the
/// case where the far side is that feature's *driver* rather than the ROM.
#[cfg(not(feature = "uart_bridge_fast"))]
const BAUD: u32 = fw_checks::checks::uart_bridge::ROM_CONSOLE_BAUD;
#[cfg(feature = "uart_bridge_fast")]
const BAUD: u32 = fw_checks::checks::uart_bridge::SPIKE_LINK_BAUD;

/// Bytes one direction may hold while its sink is away.
const QUEUE_BYTES: usize = 1024;

/// USB-Serial-JTAG's endpoint packet size. One packet in flight at a time is
/// what keeps the commit-then-await write honest: bytes are never handed to an
/// endpoint that has not been emptied.
const USB_PACKET: usize = 64;

/// How much of the USB stream is moved to UART0 per step.
const USB_TO_UART_CHUNK: usize = 256;

/// How long the UART-to-USB half waits for the host, at each of its two
/// waiting points.
///
/// Two of these is the longest the half can go without reading UART0, and
/// UART0's 128-byte hardware FIFO fills in ~11 ms at 115,200. The margin is
/// deliberate and it is why this is milliseconds rather than the 250 ms the
/// shipped image's `WritePolicy::USB_SERIAL_JTAG` uses: that policy protects a
/// device that owns its output, and this one is carrying somebody else's.
const USB_STALL_TIMEOUT: Duration = Duration::from_millis(2);

/// How long a stalled direction waits for more source bytes before it retries
/// its sink.
const DRAIN_RETRY: Duration = Duration::from_millis(2);

/// `[to_uart, to_usb]`, in RTC fast memory so a reset does not erase the
/// evidence. Zeroed on the first boot after power-on and never again.
#[esp_hal::ram(unstable(rtc_fast, persistent))]
static mut DROPS: [u32; 2] = [0; 2];

const TO_UART: usize = 0;
const TO_USB: usize = 1;

/// Read both counters and clear them: this boot reports the last run, and the
/// next one reports this run.
fn take_drops() -> (u32, u32) {
    // SAFETY: single core, and the two pump halves are cooperative tasks on one
    // executor — this runs before either is spawned.
    unsafe {
        let base = (&raw mut DROPS).cast::<u32>();
        let to_uart = base.read_volatile();
        let to_usb = base.add(1).read_volatile();
        base.write_volatile(0);
        base.add(1).write_volatile(0);
        (to_uart, to_usb)
    }
}

/// Add to one counter. Each half owns one slot, and a `u32` store is single
/// instruction on this chip, so the two halves never race for the same word.
fn note_drops(slot: usize, bytes: u32) {
    if bytes == 0 {
        return;
    }
    // SAFETY: as above; `slot` is one of the two module constants.
    unsafe {
        let cell = (&raw mut DROPS).cast::<u32>().add(slot);
        cell.write_volatile(cell.read_volatile().saturating_add(bytes));
    }
}

pub async fn run_uart_bridge(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, gpio18, _flash, gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);
    drop(gpio18);
    drop(gpio4);

    let (prev_to_uart, prev_to_usb) = take_drops();

    // The contracted header line, rendered by the same `Display` impl
    // `fw_checks::emit_header` uses — but printed rather than logged, because
    // this payload installs no logger (see the module docs).
    esp_println::println!(
        "{}{}",
        fw_checks::FW_CHECKS_HEADER_PREFIX,
        fw_checks::PayloadHeader {
            payload: "uart-bridge",
            chip: TARGET,
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        }
    );
    esp_println::println!(
        "{}",
        ReadyLine {
            baud: BAUD,
            tx_gpio: UART0_TX_GPIO,
            rx_gpio: UART0_RX_GPIO,
            prev_drop_to_uart: prev_to_uart,
            prev_drop_to_usb: prev_to_usb,
        }
    );

    let (usb_rx, usb_tx) = UsbSerialJtag::new(usb_device).into_async().split();
    let (uart_rx, uart_tx) = uart0().split();

    // From here on the bridge says nothing of its own, forever.
    join(usb_to_uart(usb_rx, uart_tx), uart_to_usb(uart_rx, usb_tx)).await;
    unreachable!("both halves loop forever")
}

/// UART0 on the chip's default U0 pads, stolen rather than threaded through
/// `init_board` — the same argument `serial::spike_uart0` makes: a harness must
/// not touch the shipped image's ownership chain, and nothing else in this
/// crate claims UART0 or GPIO16/17 (the board manifest's WS281x outputs are
/// GPIO18/20).
fn uart0() -> Uart<'static, Async> {
    // SAFETY: this harness replaces the app entry point, is the only owner of
    // UART0 and GPIO16/17 in the image, and calls this once.
    let (uart0, tx, rx) = unsafe {
        (
            esp_hal::peripherals::UART0::steal(),
            esp_hal::peripherals::GPIO16::steal(),
            esp_hal::peripherals::GPIO17::steal(),
        )
    };
    Uart::new(uart0, Config::default().with_baudrate(BAUD))
        .expect("uart-bridge: UART0 config rejected")
        .with_tx(tx)
        .with_rx(rx)
        .into_async()
}

/// USB-Serial-JTAG in, UART0 TX out. Unbounded on purpose: the sink always
/// drains, so a full queue here means the host is outrunning the line rate and
/// the right answer is back-pressure, not loss.
async fn usb_to_uart(
    mut usb_rx: UsbSerialJtagRx<'static, Async>,
    mut uart_tx: UartTx<'static, Async>,
) -> ! {
    let mut queue = ByteRing::<QUEUE_BYTES>::new();
    let mut inbuf = [0u8; USB_TO_UART_CHUNK];
    let mut outbuf = [0u8; USB_TO_UART_CHUNK];
    loop {
        // Spelled through the traits throughout this file: esp-hal's types
        // carry inherent BLOCKING `read`/`write` methods of the same names, and
        // the two differ only in whether they return a future.
        let read = Read::read(&mut usb_rx, &mut inbuf).await.unwrap_or(0);
        let step = pump(&mut queue, &inbuf[..read], &mut outbuf);
        note_drops(TO_UART, step.dropped as u32);
        if step.emitted > 0 {
            let _ = Write::write_all(&mut uart_tx, &outbuf[..step.emitted]).await;
        }
    }
}

/// UART0 RX in, USB-Serial-JTAG out. Bounded at every step: a host that has
/// closed the port must cost this half milliseconds, not the capture.
async fn uart_to_usb(
    mut uart_rx: UartRx<'static, Async>,
    mut usb_tx: UsbSerialJtagTx<'static, Async>,
) -> ! {
    let mut queue = ByteRing::<QUEUE_BYTES>::new();
    let mut inbuf = [0u8; USB_PACKET];
    let mut outbuf = [0u8; USB_PACKET];
    loop {
        // Take what UART0 has. With an empty queue there is nothing to drain,
        // so wait as long as it takes; with bytes still held, come back soon.
        let read = if queue.is_empty() {
            uart_read(&mut uart_rx, &mut inbuf, None).await
        } else {
            uart_read(&mut uart_rx, &mut inbuf, Some(DRAIN_RETRY)).await
        };

        // Only offer the sink a packet once the endpoint has been emptied.
        // While it has not, `out` is empty and the queue takes the strain.
        let room = if endpoint_free(&mut usb_tx).await {
            USB_PACKET
        } else {
            0
        };
        let step = pump(&mut queue, &inbuf[..read], &mut outbuf[..room]);
        note_drops(TO_USB, step.dropped as u32);

        if step.emitted > 0 {
            // The write commits the bytes to the endpoint before it waits for
            // the host, so a timeout here means "queued in silicon", not
            // "lost" — which is why the queue has already let them go.
            let _ = select(
                Timer::after(USB_STALL_TIMEOUT),
                Write::write_all(&mut usb_tx, &outbuf[..step.emitted]),
            )
            .await;
        }
    }
}

/// One bounded UART0 read. A hardware FIFO overrun is reported as an unknown
/// loss (`u32::MAX`, saturating) rather than a made-up byte count.
async fn uart_read(
    uart_rx: &mut UartRx<'static, Async>,
    buf: &mut [u8],
    timeout: Option<Duration>,
) -> usize {
    let result = match timeout {
        None => Read::read(uart_rx, buf).await,
        Some(t) => match select(Timer::after(t), Read::read(uart_rx, buf)).await {
            Either::First(()) => return 0,
            Either::Second(result) => result,
        },
    };
    match result {
        Ok(n) => n,
        Err(_) => {
            note_drops(TO_USB, u32::MAX);
            0
        }
    }
}

/// Has the host taken the last packet? Bounded, so "no host" costs
/// [`USB_STALL_TIMEOUT`] and not the run.
async fn endpoint_free(usb_tx: &mut UsbSerialJtagTx<'static, Async>) -> bool {
    matches!(
        select(Timer::after(USB_STALL_TIMEOUT), Write::flush(&mut *usb_tx)).await,
        Either::Second(_)
    )
}
