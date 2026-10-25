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
//! `fw_esp32_common::split_core`, from two XIAOs) and before the boot records, the mount and
//! the project load:
//!
//! 1. **Read the part, write nothing.** `RDID` and status register 2 (command
//!    `0x35`, the ROM's `esp_rom_spiflash_read_user_cmd`). The part must be on
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
//! [`switch_to_qio`] is all of it that runs with the cache suspended, from
//! RAM (~330 B). It calls only mask-ROM code, its helpers are
//! `#[inline(always)]` (the registers, the saved set and the checks are
//! `fw_esp32_common::flash_read_mode`'s, shared with the S3), and nothing in
//! it reads `.rodata` — checked with `rust-nm` and `objdump` on the linked
//! image, not trusted to the attribute.

use core::mem::MaybeUninit;

use fw_esp32_common::flash_qio::qio_ready;
use fw_esp32_common::flash_read_mode::{
    CHECK_AT, CHECK_WORDS, FREAD_DIO, FREAD_QIO, Mspi, NOT_SET_AFTER, ROM_MODE_QIO, SWITCH_ON,
    SavedReadMode, fold_differ,
};

/// The C6's flash controllers: `SPI_MEM` 0 (the cache's) at `0x6000_2000`,
/// `SPI_MEM` 1 (the one the ROM's flash routines and esp-storage drive) at
/// `0x6000_3000`.
const MSPI: Mspi = Mspi {
    spi0: 0x6000_2000,
    spi1: 0x6000_3000,
};

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
    let ctrl = MSPI.spi0_ctrl();
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
            SWITCH_ON => {
                log::info!("[flash] QIO (jedec {jedec:06x}, sr2 {sr2:02x})");
                return;
            }
            NOT_SET_AFTER => "the controller did not read back as QIO",
            _ => "the QIO read-back differed",
        },
    };
    log::info!("[flash] DIO: {why} (jedec {jedec:06x}, sr2 {sr2:02x})");
}

/// The part's JEDEC id and status register 2, from flash with interrupts
/// off — the way esp-storage's own `RDID` size probe runs on every boot. Two
/// reads; neither changes anything in the part.
fn read_part() -> (u32, u32) {
    let jedec = MSPI.rdid();
    let mut sr2 = 0;
    if unsafe { esp_rom_spiflash_read_user_cmd(&mut sr2, 0x35) } != 0 {
        sr2 = 0;
    }
    (jedec, sr2)
}

/// QIO, checked, with the cache suspended; on any doubt every register the
/// switch wrote goes back as it was.
#[esp_hal::ram]
fn switch_to_qio() -> u8 {
    unsafe {
        let autoload = Cache_Suspend_ICache();
        let saved = SavedReadMode::take(MSPI);
        // Not zeroed (that is a `memset` call): the ROM fills both, and a
        // failed read counts as a difference.
        let mut dio = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let mut qio = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let bytes = 4 * CHECK_WORDS as u32;
        let mut differ = esp_rom_spiflash_read(CHECK_AT, dio.as_mut_ptr().cast(), bytes) as u32;
        SPIMasterReadModeCnfig(ROM_MODE_QIO);
        esp_rom_spiflash_select_qio_pins(0, 0);
        differ |= esp_rom_spiflash_read(CHECK_AT, qio.as_mut_ptr().cast(), bytes) as u32;
        let differ = fold_differ(dio.assume_init_ref(), qio.assume_init_ref(), differ);
        let v = MSPI.qio_verdict(&saved, differ);
        if v != SWITCH_ON {
            saved.put_back(MSPI);
        }
        Cache_Resume_ICache(autoload);
        v
    }
}
