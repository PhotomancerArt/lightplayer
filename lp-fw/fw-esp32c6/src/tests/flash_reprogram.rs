//! ESP32-C6 `flash-reprogram`: the payload's device half.
//!
//! **This harness destroys the board's filesystem.** It owns the whole
//! `lpfs` partition and writes it for ever; it is for a board allocated to
//! be cut (CX1, `c6-expendable`, tag `sacrificial`). The build feature
//! `test_flash_reprogram` is the only gate on this side — the desk driver
//! (`scripts/emu/flash-tears-soak.sh --reprogram`) refuses any board not
//! tagged so.
//!
//! The layout, the state log, the scan, the weak-bit reads, the re-program
//! and the boot flow live in `fw_checks::checks::flash_reprogram`, where they
//! are `no_std` and tested on the host against fake parts. What stays here is
//! what needs the chip, the same as `flash_tears.rs` (whose USB handshake,
//! reset names and RDID read this copies): board init, the USB-Serial-JTAG
//! link, esp-storage, the microsecond clock and the reset reason.
//!
//! One boot: find `lpfs`; wait, writing nothing, for the host's byte; the
//! header; the boot's records; the scan-done line; then the work loop,
//! silently — it programs the region once and stops, so a cut after that
//! finds the region full (`frontier: null`).

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
use fw_checks::checks::flash_reprogram::layout::ReprogramLayout;
use fw_checks::checks::flash_reprogram::runner::{self, BootFacts, Buffers, ReprogramFlash};
use fw_checks::checks::flash_reprogram::{PAYLOAD, READY_LINE, SCAN_DONE_MARKER, WRITE_SIZE};
use fw_core::serial::SerialIo;

use crate::board::esp32c6::init::{init_board, start_runtime};
use crate::flash_layout::FlashLayout;
use crate::serial::usb_serial::Esp32UsbSerialIo;

/// How often the boot looks for the host's byte.
const POLL_MS: u64 = 5;

pub async fn run_flash_reprogram(_: embassy_executor::Spawner) -> ! {
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
        .and_then(|p| ReprogramLayout::new(p.offset, p.len));

    wait_for_host(&serial, &mut out).await;
    let _ = fw_checks::write_header(
        &mut out,
        &fw_checks::PayloadHeader {
            payload: PAYLOAD,
            chip: "esp32c6",
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );
    let Some(layout) = layout else {
        let _ = write!(
            out,
            "[flash-reprogram] REFUSED: no `lpfs` partition big enough in the flashed table \
             (flash with --partition-table lp-fw/fw-esp32c6/partitions.csv)\r\n"
        );
        idle().await
    };

    let mut flash = Flash(storage, Box::new(Bounce([0; WRITE_SIZE])));
    // 12.5 KiB, zeroed in place: a `Box::new` of the arrays would build them
    // on the main task's stack first.
    // SAFETY: `Buffers` is four byte arrays; all zeroes is a valid value.
    let mut bufs: Box<Buffers> = unsafe { Box::<Buffers>::new_zeroed().assume_init() };

    let result = {
        let mut emit = |r: &dyn fmt::Display| {
            let _ = write!(out, "{FW_CHECK_JSON_PREFIX}{r}\r\n");
        };
        let mut now_us = || Instant::now().as_micros();
        runner::boot(
            &mut flash,
            &layout,
            &mut bufs,
            facts,
            &mut now_us,
            &mut emit,
        )
    };
    let armed = match result {
        Ok(armed) => armed,
        Err(e) => {
            let _ = write!(out, "[flash-reprogram] FLASH ERROR {e:?}\r\n");
            idle().await
        }
    };
    let _ = write!(
        out,
        "{SCAN_DONE_MARKER} generation={} bytes={}\r\n",
        armed.generation,
        armed.order.len()
    );

    // The work loop. Nothing is printed from here on: the cut is the point.
    let _ = runner::work_loop(&mut flash, &armed, &mut bufs.write);
    idle().await
}

/// Wait, silently, until the host sends anything; then say so. Silently for
/// the reason `flash_tears.rs`'s `wait_for_host` gives (CX1, 2026-10-08):
/// a boot that writes before the host has the port open sends it nothing.
async fn wait_for_host(serial: &Rc<RefCell<Esp32UsbSerialIo>>, out: &mut Out) {
    let mut byte = [0u8; 16];
    while serial.borrow_mut().read_available(&mut byte).unwrap_or(0) == 0 {
        Timer::after(Duration::from_millis(POLL_MS)).await;
    }
    Timer::after(Duration::from_millis(50)).await;
    while serial.borrow_mut().read_available(&mut byte).unwrap_or(0) > 0 {}
    let _ = write!(out, "{READY_LINE}\r\n");
}

/// The flash part's JEDEC id, by the SPI1 controller's own RDID command (as
/// `flash_tears.rs` reads it).
fn flash_jedec_id() -> u32 {
    critical_section::with(|_| {
        let spi1 = esp_hal::peripherals::SPI1::regs();
        spi1.cmd().write(|w| w.flash_rdid().set_bit());
        while spi1.cmd().read().flash_rdid().bit_is_set() {}
        let id = spi1.w(0).read().buf().bits() & 0x00FF_FFFF;
        let [m, t, c, _] = id.to_le_bytes();
        u32::from_be_bytes([0, m, t, c])
    })
}

async fn idle() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

/// The reset reason, named as `flash_tears.rs` names it: a power cut is
/// `poweron`, espflash's reset `usb-uart`.
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
/// so every transfer goes through a word-aligned bounce buffer: one write
/// is at most [`WRITE_SIZE`] bytes, one esp-storage write, one ROM call.
struct Flash(FlashStorage<'static>, Box<Bounce>);

#[repr(C, align(4))]
struct Bounce([u8; WRITE_SIZE]);

impl ReprogramFlash for Flash {
    type Error = FlashStorageError;

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error> {
        let mut bounce = Bounce([0; WRITE_SIZE]);
        let mut at = addr;
        for chunk in buf.chunks_mut(WRITE_SIZE) {
            let n = chunk.len().next_multiple_of(4);
            ReadNorFlash::read(&mut self.0, at, &mut bounce.0[..n])?;
            chunk.copy_from_slice(&bounce.0[..chunk.len()]);
            at += chunk.len() as u32;
        }
        Ok(())
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        NorFlash::erase(&mut self.0, from, to)
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error> {
        debug_assert!(data.len() <= WRITE_SIZE && data.len() % 4 == 0);
        let bounce = &mut self.1.0[..data.len()];
        bounce.copy_from_slice(data);
        NorFlash::write(&mut self.0, addr, bounce)
    }
}
