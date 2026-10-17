//! The core reads its flash four bits at a time (QIO) when the part is ready
//! for it, and says which mode it chose in one boot line.
//!
//! The C6 boots in DIO and stays there through the mask ROM, the second-stage
//! bootloader (the stock ESP-IDF one or our own) and the split image's loader:
//! the image header says DIO, and it must — a QIO header makes the ROM's own
//! load fail (docs/reports/2026-10-17-c6-flash-mode-and-first-light.md). The
//! bus already runs at 80 MHz by the time the core starts, so the one
//! remaining step is the bus width. On CX1 QIO took first light 654 → 564 ms
//! and a 32-byte cache line fill 2,167 → 1,315 ns.
//!
//! [`enter`] runs once, right after esp-storage's `FlashStorage::new` (whose
//! own `RDID` size probe must come before any ROM flash access — the rule in
//! `ota/mod.rs`, from two XIAOs) and before the boot records, the mount and
//! the project load:
//!
//! 1. **Read the part, write nothing.** `RDID` and status register 2 (command
//!    `0x35`), with the instruction cache suspended. The part must be on
//!    `fw_esp32_common::flash_qio::QIO_PARTS` and its QE bit must already be
//!    set; otherwise the core stays in DIO and says why. **No status register
//!    is ever written here.** That is why this does not call the ROM's
//!    `esp_rom_spiflash_config_readmode`: in the C6's mask ROM
//!    (`esp32c6-20220919`, `lp-emu/esp/roms/esp32c6_rev0_rom.elf`) that
//!    function, asked for QIO, calls `Enable_QMode`, which reads SR2 itself
//!    and *writes* the status register when it reads QE clear. The ROM's
//!    `SPIMasterReadModeCnfig` (`0x4000_01a0`) makes the same controller
//!    writes with no QE step: SPI0's and SPI1's `SPI_MEM_CTRL` read-mode bits,
//!    then SPI0's read command (`0xEB`), address length (24 + 8 mode bits),
//!    dummy cycles, and mode byte (`SPI_MEM_RD_STATUS.WB_MODE`, 0 — so the
//!    part never enters a continuous-read mode that a reset would leave it
//!    in). `esp_rom_spiflash_select_qio_pins(0, 0)` then gives the WP and HD
//!    pads to the flash controller, as ESP-IDF's bootloader does after it.
//! 2. **Check before the cache resumes.** The controller must have been in
//!    DIO, and afterwards must read QIO, command `0xEB`, a 32-bit address
//!    phase and two more dummy cycles than DIO had; and 32 bytes at flash
//!    `0x0` read over SPI1 in QIO must equal the same bytes read in DIO just
//!    before. Anything else puts every register the switch wrote back as it
//!    was, and the core stays in DIO.
//!
//! Nothing persists. The mode is controller state; every reset — power-on,
//! the firmware's own `LP_SW_HPSYS` reset (OTA, reboot), a watchdog — runs
//! the ROM again, which loads the bootloader in the header's DIO, and the
//! part's QE bit was set before this core existed. An update core that fails
//! here fails its trial, and the loader gives the previous core back.
//!
//! The two RAM functions are all of it that runs with the cache suspended.
//! They call only mask-ROM code, their helpers are `#[inline(always)]`, and
//! nothing in them reads `.rodata` — checked with `rust-nm` and `objdump` on
//! the linked image, not trusted to the attribute.

use core::mem::MaybeUninit;
use core::ptr::{read_volatile, write_volatile};

use fw_esp32_common::flash_qio::qio_ready;

/// `SPI_MEM` 0, the cache's flash controller.
const SPI0: usize = 0x6000_2000;
/// `SPI_MEM` 1, the one the ROM's flash routines (and esp-storage) drive.
const SPI1: usize = 0x6000_3000;
// Register offsets, the same in both blocks (C6 TRM, "SPI0/1 Register
// Summary").
const CMD: usize = 0x00;
const CTRL: usize = 0x08;
const USER: usize = 0x18;
const USER1: usize = 0x1c;
const USER2: usize = 0x20;
/// SPI0: `SPI_MEM_RD_STATUS`, whose `WB_MODE` field is the QIO mode byte.
const RD_STATUS: usize = 0x2c;
/// SPI0: `SPI_MEM_CACHE_FCTRL`.
const FCTRL: usize = 0x3c;
const W0: usize = 0x58;

/// `SPI_MEM_CMD.SPI_MEM_FLASH_RDID`.
const CMD_RDID: u32 = 1 << 28;
/// `SPI_MEM_CTRL.SPI_MEM_FREAD_DIO` / `SPI_MEM_FREAD_QIO`.
const FREAD_DIO: u32 = 1 << 23;
const FREAD_QIO: u32 = 1 << 24;
/// `SPIMasterReadModeCnfig`'s mode argument (the ROM's read-mode enum: QIO,
/// QOUT, DIO, DOUT, FASTRD, SLOWRD).
const ROM_MODE_QIO: u32 = 0;
/// Where the read-back check reads: the start of the flash, which holds a
/// bootloader image under every door (never erased, never all ones).
const CHECK_AT: u32 = 0;
const CHECK_WORDS: usize = 8;

/// [`switch_to_qio`]'s answers.
const QIO_ON: u8 = 0;
const NOT_QIO_AFTER: u8 = 1;
const READ_BACK_DIFFERED: u8 = 2;

unsafe extern "C" {
    fn Cache_Suspend_ICache() -> u32;
    fn Cache_Resume_ICache(autoload: u32);
    fn esp_rom_spiflash_read_user_cmd(status: *mut u32, cmd: u8) -> i32;
    fn esp_rom_spiflash_read(src_addr: u32, data: *mut u32, len: u32) -> i32;
    fn SPIMasterReadModeCnfig(mode: u32) -> i32;
    fn esp_rom_spiflash_select_qio_pins(wp_gpio_num: u8, ishspi: u32);
}

/// Choose the read mode, once, and say which in one line.
pub fn enter() {
    let ctrl = rd(SPI0 + CTRL);
    if ctrl & FREAD_QIO != 0 {
        log::info!("[flash] QIO, set before the core");
        return;
    }
    if ctrl & FREAD_DIO == 0 {
        log::info!("[flash] stays as booted: not DIO (ctrl {ctrl:08x})");
        return;
    }
    let (jedec, sr2) = critical_section::with(|_| read_part());
    let why = match qio_ready(jedec, sr2) {
        Err(why) => why.words(),
        Ok(()) => match critical_section::with(|_| switch_to_qio()) {
            QIO_ON => {
                log::info!("[flash] QIO (jedec {jedec:06x}, sr2 {sr2:02x})");
                return;
            }
            NOT_QIO_AFTER => "the controller did not read back as QIO",
            _ => "the QIO read-back differed",
        },
    };
    log::info!("[flash] DIO: {why} (jedec {jedec:06x}, sr2 {sr2:02x})");
}

/// The part's JEDEC id and status register 2, read with the cache suspended.
#[esp_hal::ram]
fn read_part() -> (u32, u32) {
    unsafe {
        let autoload = Cache_Suspend_ICache();
        wr(SPI1 + CMD, CMD_RDID);
        while rd(SPI1 + CMD) & CMD_RDID != 0 {}
        let jedec = rd(SPI1 + W0) & 0x00ff_ffff;
        let mut sr2 = 0;
        if esp_rom_spiflash_read_user_cmd(&mut sr2, 0x35) != 0 {
            sr2 = 0;
        }
        Cache_Resume_ICache(autoload);
        (jedec, sr2)
    }
}

/// QIO, checked, with the cache suspended; on any doubt every register the
/// switch wrote goes back as it was.
#[esp_hal::ram]
fn switch_to_qio() -> u8 {
    unsafe {
        let autoload = Cache_Suspend_ICache();
        let saved = Saved::take();
        // Not zeroed (that is a `memset` call): the ROM fills both, and a
        // failed read counts as a difference.
        let mut dio = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let mut qio = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let bytes = 4 * CHECK_WORDS as u32;
        let mut differ = esp_rom_spiflash_read(CHECK_AT, dio.as_mut_ptr().cast(), bytes) as u32;
        SPIMasterReadModeCnfig(ROM_MODE_QIO);
        esp_rom_spiflash_select_qio_pins(0, 0);
        differ |= esp_rom_spiflash_read(CHECK_AT, qio.as_mut_ptr().cast(), bytes) as u32;
        let (dio, qio) = (dio.assume_init_ref(), qio.assume_init_ref());
        let mut i = 0;
        while i < CHECK_WORDS {
            differ |= dio[i] ^ qio[i];
            i += 1;
        }

        let user1 = rd(SPI0 + USER1);
        let verdict = if rd(SPI0 + CTRL) & (FREAD_DIO | FREAD_QIO) != FREAD_QIO
            || rd(SPI1 + CTRL) & (FREAD_DIO | FREAD_QIO) != FREAD_QIO
            || rd(SPI0 + USER2) & 0xffff != 0xeb
            || user1 >> 26 != 31
            || user1 & 0x3f != (saved.spi0_user1 & 0x3f) + 2
        {
            NOT_QIO_AFTER
        } else if differ != 0 {
            READ_BACK_DIFFERED
        } else {
            QIO_ON
        };
        if verdict != QIO_ON {
            saved.put_back();
        }
        Cache_Resume_ICache(autoload);
        verdict
    }
}

/// The registers `SPIMasterReadModeCnfig` writes, as they were. Spelled out
/// rather than a table: a table would be `.rodata`, in flash, and these run
/// with the cache suspended.
struct Saved {
    spi0_ctrl: u32,
    spi0_user: u32,
    spi0_user1: u32,
    spi0_user2: u32,
    spi0_rd_status: u32,
    spi0_fctrl: u32,
    spi1_ctrl: u32,
}

impl Saved {
    #[inline(always)]
    fn take() -> Self {
        Saved {
            spi0_ctrl: rd(SPI0 + CTRL),
            spi0_user: rd(SPI0 + USER),
            spi0_user1: rd(SPI0 + USER1),
            spi0_user2: rd(SPI0 + USER2),
            spi0_rd_status: rd(SPI0 + RD_STATUS),
            spi0_fctrl: rd(SPI0 + FCTRL),
            spi1_ctrl: rd(SPI1 + CTRL),
        }
    }

    #[inline(always)]
    fn put_back(&self) {
        wr(SPI0 + CTRL, self.spi0_ctrl);
        wr(SPI0 + USER, self.spi0_user);
        wr(SPI0 + USER1, self.spi0_user1);
        wr(SPI0 + USER2, self.spi0_user2);
        wr(SPI0 + RD_STATUS, self.spi0_rd_status);
        wr(SPI0 + FCTRL, self.spi0_fctrl);
        wr(SPI1 + CTRL, self.spi1_ctrl);
    }
}

#[inline(always)]
fn rd(addr: usize) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

#[inline(always)]
fn wr(addr: usize, value: u32) {
    unsafe { write_volatile(addr as *mut u32, value) }
}
