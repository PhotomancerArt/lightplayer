//! The core reads its flash at 80 MHz, and four bits at a time (QIO) when the
//! part is one an S3 sitting has read, and says which in one boot line. The
//! C6's `fw-esp32c6/src/flash_qio.rs` on this chip, with one step the C6 does
//! not need: the clock.
//!
//! The S3 boots its flash at **DIO, 40 MHz**: the image header says so
//! (`e9 07 02 30`: mode 2 = DIO, frequency nibble 0 = 40 MHz), the mask ROM,
//! espflash's bundled ESP-IDF bootloader and the split loader all read with
//! it, and every reset starts there again. The header stays that way (a QIO
//! header bricks the ROM's own load on the C6, and the S3 has only the IDF's
//! door). Nothing re-clocked the flash after the boot chain until this: the
//! S3 PSRAM spike (2026-10-21, FS3, silicon) found that esp-hal's PSRAM init
//! re-clocks it to 80 MHz as a side effect, and that alone took a
//! 620-pixel project from 22 to 28 fps, its shader compile from 42.6 to
//! 26.0 ms, and its load from 227 to 164 ms.
//!
//! [`enter`] runs once, right after esp-storage's `FlashStorage::new` (whose
//! own `RDID` size probe must come before any ROM flash access), before the
//! partition table, the boot records and the mount:
//!
//! 1. **80 MHz** ([`switch_to_80mhz`]): the MSPI core clock at 80 MHz
//!    (`SPI_MEM_CORE_CLK_SEL` 0) and the flash clock at the core clock
//!    (`SPI_MEM_CLOCK.CLK_EQU_SYSCLK`) on SPI0 (the cache) and SPI1 (the ROM's
//!    flash routines, esp-storage). These are the writes esp-hal's PSRAM init
//!    makes with its defaults (`psram/esp32s3.rs`,
//!    `mspi_timing_enter_high_speed_mode`: core clock 80 MHz, flash divider
//!    1), the ones the spike ran on FS3; like there, no dummy cycle and no
//!    timing-tuning register changes. Controller state only: nothing is
//!    written to the part.
//! 2. **QIO** on the parts `fw_esp32_common::flash_qio::ESP32S3_QIO_PARTS`
//!    names, with QE already set, through the ROM's `SPIMasterReadModeCnfig`
//!    — the C6's switch, the same checks (`fw_esp32_common::flash_read_mode`).
//!    The list is empty today: FS3's part has not had its id read on
//!    silicon, and a C6's evidence is not evidence for this controller. So
//!    every S3 reads in DIO, and the boot line says why. **No status
//!    register is ever written** (the C6 ADR's reason: `config_readmode`'s
//!    `Enable_QMode` writes it when it reads QE clear, and a status write is
//!    a non-volatile write).
//!
//! Each step is checked before the caches resume, from RAM: the controller
//! must read back as the step left it, and 32 bytes at flash `0x0` read over
//! SPI1 after the step must equal the same bytes read before. Anything else
//! puts every register the step wrote back as it was. The S3 has two caches
//! on SPI0 (instruction and data), so both are suspended.
//!
//! Nothing persists: the clock and the mode are controller state, and every
//! reset runs the ROM again in the header's DIO at 40 MHz. An update core
//! that fails here fails its trial, and the loader gives the previous core
//! back.
//!
//! The emulator (`lp-emu-esp32s3`) remembers both controllers' clock and
//! read-mode registers and charges no time for the flash clock, so an
//! emulated boot shows the step taken (the boot line), never its speed.

use core::mem::MaybeUninit;

use fw_esp32_common::flash_qio::{ESP32S3_QIO_PARTS, qio_ready_on};
use fw_esp32_common::flash_read_mode::{
    CHECK_AT, CHECK_WORDS, FREAD_DIO, FREAD_QIO, Mspi, NOT_SET_AFTER, READ_BACK_DIFFERED,
    ROM_MODE_QIO, SWITCH_ON, SavedReadMode, fold_differ, verdict_words,
};

/// The S3's flash controllers: SPI0 (the cache's) at `0x6000_3000` and SPI1
/// at `0x6000_2000` — the other way round from the C6
/// (`lp-emu-esp32s3`'s `periph/spi0.rs` cites the PAC).
const MSPI: Mspi = Mspi {
    spi0: 0x6000_3000,
    spi1: 0x6000_2000,
};

/// `SPI_MEM_CLOCK`, both controllers (`esp32s3` 0.35.2 PAC: `clock` at
/// `0x14`).
const CLOCK: usize = 0x14;
/// `SPI_MEM_CLOCK.CLK_EQU_SYSCLK` (bit 31): the flash clock is the MSPI core
/// clock, undivided.
const CLK_EQU_SYSCLK: u32 = 1 << 31;
/// SPI0's `SPI_MEM_CORE_CLK_SEL` (`0xec`), shared by both controllers. Bits
/// 1:0: 0 = 80 MHz, 1 = 120, 2 = 160, 3 = 240 (esp-hal's
/// `spi0_timing_config_set_core_clock`).
const CORE_CLK_SEL: usize = 0xec;
const CORE_CLK_MASK: u32 = 0b11;
/// `CORE_CLK_SEL` for an 80 MHz MSPI core clock.
const CORE_CLK_80M: u32 = 0;

unsafe extern "C" {
    /// The mask ROM's `Cache_Suspend_ICache` (`0x4004_f3a0`, through the jump
    /// table's `0x4000_189c`), which esp-rom-sys's S3 script provides as
    /// `rom_Cache_Suspend_ICache` (it keeps the plain name free for a patched
    /// wrapper, which nothing in this firmware links). The ROM routine waits
    /// for the ICache to go idle (it polls `EXTMEM` `0x600C_4094` bit 1,
    /// `4004f3d8..dd`) before it returns, as the C6's does.
    #[link_name = "rom_Cache_Suspend_ICache"]
    fn Cache_Suspend_ICache() -> u32;
    fn Cache_Resume_ICache(autoload: u32);
    fn Cache_Suspend_DCache() -> u32;
    fn Cache_Resume_DCache(autoload: u32);
    fn esp_rom_spiflash_read_user_cmd(status: *mut u32, cmd: u8) -> i32;
    fn esp_rom_spiflash_read(src_addr: u32, data: *mut u32, len: u32) -> i32;
    fn SPIMasterReadModeCnfig(mode: u32) -> i32;
    fn esp_rom_spiflash_select_qio_pins(wp_gpio_num: u8, ishspi: u32);
}

/// Choose the flash clock and the read mode, once, and say which in one line.
pub fn enter() {
    let clock = if at_80mhz() {
        "80 MHz, set before the core"
    } else {
        match critical_section::with(|_| switch_to_80mhz()) {
            SWITCH_ON => "80 MHz",
            v => {
                esp_println::println!("[flash] 80 MHz refused: {}", verdict_words(v));
                "40 MHz"
            }
        }
    };
    let ctrl = MSPI.spi0_ctrl();
    if ctrl & FREAD_QIO != 0 {
        esp_println::println!("[flash] {clock} QIO, set before the core");
        return;
    }
    if ctrl & FREAD_DIO == 0 {
        esp_println::println!("[flash] {clock}, mode as booted: not DIO (ctrl {ctrl:08x})");
        return;
    }
    let (jedec, sr2) = critical_section::with(|_| read_part());
    let why = match qio_ready_on(ESP32S3_QIO_PARTS, jedec, sr2) {
        Err(why) => why.words(),
        Ok(()) => match critical_section::with(|_| switch_to_qio()) {
            SWITCH_ON => {
                esp_println::println!("[flash] {clock} QIO (jedec {jedec:06x}, sr2 {sr2:02x})");
                return;
            }
            v => verdict_words(v),
        },
    };
    esp_println::println!("[flash] {clock} DIO: {why} (jedec {jedec:06x}, sr2 {sr2:02x})");
}

/// Whether both controllers already run the flash at an 80 MHz core clock.
///
/// A `psram` image asks the frequency instead (M10): its PSRAM init, which
/// runs before this, leaves the MSPI core clock at 160 MHz with the flash and
/// PSRAM each divided by two — flash at 80 MHz already. The register pattern
/// below would not see that, and the switch would take the core clock back
/// to 80 MHz and PSRAM down to 40 with it.
fn at_80mhz() -> bool {
    #[cfg(feature = "psram")]
    {
        let c = clocks();
        if c.flash_spi0 == 80 && c.flash_spi1 == 80 {
            return true;
        }
    }
    Mspi::rd(MSPI.spi0 + CORE_CLK_SEL) & CORE_CLK_MASK == CORE_CLK_80M
        && Mspi::rd(MSPI.spi0 + CLOCK) & CLK_EQU_SYSCLK != 0
        && Mspi::rd(MSPI.spi1 + CLOCK) & CLK_EQU_SYSCLK != 0
}

/// The part's JEDEC id and status register 2, with interrupts off. Two
/// reads; neither changes anything in the part.
fn read_part() -> (u32, u32) {
    let jedec = MSPI.rdid();
    let mut sr2 = 0;
    if unsafe { esp_rom_spiflash_read_user_cmd(&mut sr2, 0x35) } != 0 {
        sr2 = 0;
    }
    (jedec, sr2)
}

/// 80 MHz, checked, with both caches suspended; on any doubt the three
/// registers it wrote go back as they were.
#[esp_hal::ram]
fn switch_to_80mhz() -> u8 {
    unsafe {
        let i_autoload = Cache_Suspend_ICache();
        let d_autoload = Cache_Suspend_DCache();
        let core_clk = Mspi::rd(MSPI.spi0 + CORE_CLK_SEL);
        let spi0_clock = Mspi::rd(MSPI.spi0 + CLOCK);
        let spi1_clock = Mspi::rd(MSPI.spi1 + CLOCK);
        // Not zeroed (that is a `memset` call): the ROM fills both, and a
        // failed read counts as a difference.
        let mut before = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let mut after = MaybeUninit::<[u32; CHECK_WORDS]>::uninit();
        let bytes = 4 * CHECK_WORDS as u32;
        let mut differ = esp_rom_spiflash_read(CHECK_AT, before.as_mut_ptr().cast(), bytes) as u32;
        Mspi::wr(
            MSPI.spi0 + CORE_CLK_SEL,
            (core_clk & !CORE_CLK_MASK) | CORE_CLK_80M,
        );
        Mspi::wr(MSPI.spi0 + CLOCK, spi0_clock | CLK_EQU_SYSCLK);
        Mspi::wr(MSPI.spi1 + CLOCK, spi1_clock | CLK_EQU_SYSCLK);
        differ |= esp_rom_spiflash_read(CHECK_AT, after.as_mut_ptr().cast(), bytes) as u32;
        let differ = fold_differ(before.assume_init_ref(), after.assume_init_ref(), differ);
        let v = if Mspi::rd(MSPI.spi0 + CORE_CLK_SEL) & CORE_CLK_MASK != CORE_CLK_80M
            || Mspi::rd(MSPI.spi0 + CLOCK) & CLK_EQU_SYSCLK == 0
            || Mspi::rd(MSPI.spi1 + CLOCK) & CLK_EQU_SYSCLK == 0
        {
            NOT_SET_AFTER
        } else if differ != 0 {
            READ_BACK_DIFFERED
        } else {
            SWITCH_ON
        };
        if v != SWITCH_ON {
            Mspi::wr(MSPI.spi1 + CLOCK, spi1_clock);
            Mspi::wr(MSPI.spi0 + CLOCK, spi0_clock);
            Mspi::wr(MSPI.spi0 + CORE_CLK_SEL, core_clk);
        }
        Cache_Resume_DCache(d_autoload);
        Cache_Resume_ICache(i_autoload);
        v
    }
}

/// QIO, checked, with both caches suspended; on any doubt every register the
/// switch wrote goes back as it was. The C6's `switch_to_qio`, on this
/// chip's controllers. Unreached until [`ESP32S3_QIO_PARTS`] names a part.
#[esp_hal::ram]
fn switch_to_qio() -> u8 {
    unsafe {
        let i_autoload = Cache_Suspend_ICache();
        let d_autoload = Cache_Suspend_DCache();
        let saved = SavedReadMode::take(MSPI);
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
        Cache_Resume_DCache(d_autoload);
        Cache_Resume_ICache(i_autoload);
        v
    }
}

/// `SPI_MEM_SRAM_CLK` (`0x50`, SPI0): PSRAM's clock, the same shape as
/// [`CLOCK`] (bits 23:16 the divider less one, bit 31 undivided).
#[cfg(feature = "psram")]
const SRAM_CLK: usize = 0x50;

/// The MSPI clocks as the controllers hold them, in MHz.
#[cfg(feature = "psram")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MspiClocks {
    pub core: u32,
    pub flash_spi0: u32,
    pub flash_spi1: u32,
    pub psram: u32,
}

/// Read the four clocks off the registers (esp-hal's own encodings:
/// `spi0_timing_config_set_core_clock`, `…_set_flash_clock`,
/// `…_set_psram_clock`).
#[cfg(feature = "psram")]
pub fn clocks() -> MspiClocks {
    let core = match Mspi::rd(MSPI.spi0 + CORE_CLK_SEL) & CORE_CLK_MASK {
        0 => 80,
        1 => 120,
        2 => 160,
        _ => 240,
    };
    let divided = |reg: u32| {
        if reg & CLK_EQU_SYSCLK != 0 {
            core
        } else {
            core / (((reg >> 16) & 0xff) + 1)
        }
    };
    MspiClocks {
        core,
        flash_spi0: divided(Mspi::rd(MSPI.spi0 + CLOCK)),
        flash_spi1: divided(Mspi::rd(MSPI.spi1 + CLOCK)),
        psram: divided(Mspi::rd(MSPI.spi0 + SRAM_CLK)),
    }
}

/// The boot line that states where the two writers of these clocks — PSRAM's
/// init and [`enter`] — left them (a `psram` image; ADR
/// `2026-10-22-s3-psram.md`, point 10).
#[cfg(feature = "psram")]
pub fn log_clocks() {
    let c = clocks();
    esp_println::println!(
        "[INIT] mspi clocks: core {} MHz, flash {} MHz (SPI0) {} MHz (SPI1), psram {} MHz",
        c.core,
        c.flash_spi0,
        c.flash_spi1,
        c.psram
    );
}
