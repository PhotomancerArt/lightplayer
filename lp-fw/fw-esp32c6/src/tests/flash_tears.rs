//! ESP32-C6 `flash-tears`: the payload's device half.
//!
//! **This harness destroys the board's filesystem.** It owns the first 18
//! sectors of `lpfs` and writes them for ever; it is for a board allocated to
//! be cut (CX1, `c6-expendable`, tag `sacrificial`). The build feature
//! `test_flash_tears` is the only gate on this side — the desk driver
//! (`scripts/emu/flash-tears-soak.sh`) refuses any board not tagged so.
//!
//! The layout, the patterns, the journal, the classifier, the records and the
//! whole boot flow live in `fw_checks::checks::flash_tears`, where they are
//! `no_std` and tested on the host against a fake part with cuts. What stays
//! here is what needs the chip: board init, the USB-Serial-JTAG link, the
//! flash driver (esp-storage over the mask ROM's SPI flash routines), the
//! microsecond clock and the reset reason.
//!
//! One boot:
//!
//! 1. find `lpfs` in the flashed partition table;
//! 2. wait, writing nothing, until the host sends a byte — after a power cut
//!    the board boots before the host has the port open again, and a write
//!    nobody reads latches the link into dropping (see [`wait_for_host`]);
//! 3. the header, then the scan's records (nothing is written before the
//!    scan reads);
//! 4. the repair and one timed work cycle, with their records;
//! 5. the scan-done line, then the work loop, silently, until the power goes.
//!
//! Built with `test_flash_tears_unaligned` instead, the same harness is the
//! `flash-tears-unaligned` payload: every work cycle programs its sector in
//! 16–272-byte writes starting off every 32-byte boundary
//! (`fw_checks::checks::flash_tears::program_plan`), each one esp-storage
//! write and so one call into the mask ROM, to measure whether a torn
//! prefix stops relative to the write's address or on absolute 32-byte
//! boundaries. Only the plan and the payload name change.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::fmt::{self, Write as _};

use embassy_time::{Duration, Instant, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_hal::rtc_cntl::SocResetReason;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_storage::{FlashStorage, FlashStorageError};
use fw_checks::FW_CHECK_JSON_PREFIX;
use fw_checks::checks::flash_tears::layout::TearsLayout;
use fw_checks::checks::flash_tears::program_plan::{MAX_WRITE, ProgramMode};
use fw_checks::checks::flash_tears::runner::{self, BootFacts, ScanBuffers, TearsFlash};
use fw_checks::checks::flash_tears::{PAGE_SIZE, READY_LINE, SCAN_DONE_MARKER};
use fw_core::serial::SerialIo;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::flash_layout::FlashLayout;
// Through the module rather than the re-export, as `cycle_probe` does: the
// re-export is gated to a named list of harnesses.
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// How often the boot looks for the host's byte.
const POLL_MS: u64 = 5;

#[cfg(all(feature = "test_flash_tears", feature = "test_flash_tears_unaligned"))]
compile_error!("`test_flash_tears` and `test_flash_tears_unaligned` are two payloads; build one");

/// How this image's work cycles program a sector: its build feature names it.
const MODE: ProgramMode = if cfg!(feature = "test_flash_tears_unaligned") {
    ProgramMode::Unaligned
} else {
    ProgramMode::Pages
};

pub async fn run_flash_tears(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, usb_device, _gpio18, flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);
    let facts = BootFacts {
        reset: reset_name(esp_hal::system::reset_reason()),
        mac: esp_hal::efuse::base_mac_address()
            .as_bytes()
            .try_into()
            .unwrap_or([0; 6]),
        flash_id: flash_jedec_id(),
    };

    let serial = Rc::new(RefCell::new(Esp32UsbSerialIo::new(UsbSerialJtag::new(
        usb_device,
    ))));
    let mut out = Out(serial.clone());

    let mut storage = FlashStorage::new(flash);
    let layout = FlashLayout::locate(&mut storage)
        .lpfs
        .and_then(|p| TearsLayout::new(p.offset, p.len))
        .map(|l| l.with_mode(MODE));

    wait_for_host(&serial, &mut out).await;
    let _ = fw_checks::write_header(
        &mut out,
        &fw_checks::PayloadHeader {
            payload: MODE.payload(),
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    let Some(layout) = layout else {
        let _ = write!(
            out,
            "[flash-tears] REFUSED: no `lpfs` partition big enough in the flashed table \
             (flash with --partition-table lp-fw/fw-esp32c6/partitions.csv)\r\n"
        );
        idle().await
    };

    // SAFETY: `Bounce` is a byte array; all zeroes is a valid value.
    let mut flash = Flash(storage, unsafe {
        Box::<Bounce>::new_zeroed().assume_init()
    });
    // 20 KiB, zeroed in place: a `Box::new` of the arrays would build them
    // on the main task's stack first.
    // SAFETY: `ScanBuffers` is five byte arrays; all zeroes is a valid value.
    let mut bufs: Box<ScanBuffers> = unsafe { Box::<ScanBuffers>::new_zeroed().assume_init() };

    let next = match boot(&mut flash, &layout, &mut bufs, facts, &mut out) {
        Ok(next) => next,
        Err(e) => {
            let _ = write!(out, "[flash-tears] FLASH ERROR {e:?}\r\n");
            idle().await
        }
    };
    let _ = write!(out, "{SCAN_DONE_MARKER} next={next}\r\n");

    // The work loop. Nothing is printed from here on: the cut is the point.
    let err = runner::run_cycles(&mut flash, &layout, next, u32::MAX - next, &mut bufs.new);
    let _ = write!(out, "[flash-tears] work loop stopped: {err:?}\r\n");
    idle().await
}

/// Scan, repair and one timed cycle, every record printed as it is made.
/// Returns the cycle the work loop resumes at.
fn boot(
    flash: &mut Flash,
    layout: &TearsLayout,
    bufs: &mut ScanBuffers,
    facts: BootFacts<'_>,
    out: &mut Out,
) -> Result<u32, FlashStorageError> {
    let mut emit = |r: &dyn fmt::Display| {
        let _ = write!(out, "{FW_CHECK_JSON_PREFIX}{r}\r\n");
    };
    let found = runner::scan(flash, layout, bufs, facts, &mut emit)?;
    let next = runner::prepare(flash, layout, &found, bufs, &mut emit)?;
    let mut now_us = || Instant::now().as_micros();
    let timing = runner::timed_cycle(flash, layout, next, &mut bufs.new, &mut now_us)?;
    emit(&timing);
    Ok(next + 1)
}

/// Wait, silently, until the host sends anything; then say so.
///
/// Silently, and that is the fix for a desk finding (2026-10-08, CX1). When
/// this loop printed a ready line every 250 ms, a boot after a real power
/// cut sent the host **nothing** — not one byte in 15 s with the port open —
/// while the host's bytes still arrived here (the journal showed the work
/// loop resuming). The likely mechanism: the boot enumerates USB afresh, the
/// first line fills the endpoint buffer before the host has the port open,
/// times out, and latches `Esp32UsbSerialIo` into dropping. Whatever the
/// mechanism, a boot that writes nothing until it has heard from the host
/// was captured every time.
async fn wait_for_host(serial: &Rc<RefCell<Esp32UsbSerialIo>>, out: &mut Out) {
    let mut byte = [0u8; 16];
    while serial.borrow_mut().read_available(&mut byte).unwrap_or(0) == 0 {
        Timer::after(Duration::from_millis(POLL_MS)).await;
    }
    // Let the host's burst finish, and drop it.
    Timer::after(Duration::from_millis(50)).await;
    while serial.borrow_mut().read_available(&mut byte).unwrap_or(0) > 0 {}
    let _ = write!(out, "{READY_LINE}\r\n");
}

/// The flash part's JEDEC id, by the SPI1 controller's own RDID command:
/// esp-storage's size probe (`hardware::get_flash_size`) reads it the same
/// way, before any write.
fn flash_jedec_id() -> u32 {
    critical_section::with(|_| {
        let spi1 = esp_hal::peripherals::SPI1::regs();
        spi1.cmd().write(|w| w.flash_rdid().set_bit());
        while spi1.cmd().read().flash_rdid().bit_is_set() {}
        let id = spi1.w(0).read().buf().bits() & 0x00FF_FFFF;
        // The part sends manufacturer, type, capacity; the word holds them
        // first-byte-lowest. Print them in the order the part sent them.
        let [m, t, c, _] = id.to_le_bytes();
        u32::from_be_bytes([0, m, t, c])
    })
}

async fn idle() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// The reset reason, as the `ft-boot` record names it. A power cut is
/// `poweron`; espflash's reset is `usb-uart`; a brownout detector that fired
/// before the supply went away is `brownout`.
fn reset_name(reason: Option<SocResetReason>) -> &'static str {
    match reason {
        Some(SocResetReason::ChipPowerOn) => "poweron",
        Some(SocResetReason::SysBrownOut) => "brownout",
        Some(SocResetReason::CoreUsbUart) => "usb-uart",
        Some(SocResetReason::CoreUsbJtag) => "usb-jtag",
        Some(SocResetReason::CoreSw | SocResetReason::Cpu0Sw) => "software",
        Some(
            SocResetReason::CoreRtcWdt
            | SocResetReason::Cpu0RtcWdt
            | SocResetReason::SysRtcWdt
            | SocResetReason::SysSuperWdt
            | SocResetReason::CoreMwdt0
            | SocResetReason::CoreMwdt1
            | SocResetReason::Cpu0Mwdt0
            | SocResetReason::Cpu0Mwdt1,
        ) => "watchdog",
        Some(_) => "other",
        None => "unknown",
    }
}

/// Text to the USB link, best effort.
struct Out(Rc<RefCell<Esp32UsbSerialIo>>);

impl fmt::Write for Out {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let _ = self.0.borrow_mut().write(s.as_bytes());
        Ok(())
    }
}

/// esp-storage as the payload's flash. Built with `panic-unaligned-buffer`,
/// so every transfer goes through a word-aligned bounce buffer: a journal
/// entry is a 16-byte stack array with no alignment of its own. Programs go
/// through the boxed one, which holds the longest write either plan makes
/// (272 bytes under the unaligned plan) as ONE esp-storage write — one
/// call into the ROM, which is what the unaligned payload measures.
struct Flash(FlashStorage<'static>, Box<Bounce>);

#[repr(C, align(4))]
struct Bounce([u8; MAX_WRITE]);

#[repr(C, align(4))]
struct ReadBounce([u8; PAGE_SIZE]);

impl TearsFlash for Flash {
    type Error = FlashStorageError;

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        let mut bounce = ReadBounce([0; PAGE_SIZE]);
        let mut at = addr;
        for chunk in buf.chunks_mut(PAGE_SIZE) {
            let n = chunk.len().next_multiple_of(4);
            ReadNorFlash::read(&mut self.0, at, &mut bounce.0[..n])?;
            chunk.copy_from_slice(&bounce.0[..chunk.len()]);
            at += chunk.len() as u32;
        }
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), Self::Error> {
        NorFlash::erase(&mut self.0, addr, addr + 4096)
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        debug_assert!(data.len() <= MAX_WRITE && data.len() % 4 == 0);
        let bounce = &mut self.1.0[..data.len()];
        bounce.copy_from_slice(data);
        NorFlash::write(&mut self.0, addr, bounce)
    }
}
