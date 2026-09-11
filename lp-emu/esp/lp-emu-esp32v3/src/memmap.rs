//! The classic ESP32's (v3, LX6) address space.
//!
//! Provenance: every number here is read from
//! `third_party/esp-hal/ld/esp32/memory.x`, the `REGION_ALIAS` lines in
//! `third_party/esp-hal/build.rs:110-116`, the **hardware-measured** facts in
//! `lp-emu/lp-xt-emu/src/board.rs:156-185`, the vendored ROM ELF's own
//! program headers (read in M3's discovery, `m3/notes.md` §2) and
//! `esp32-0.40.2/src/lib.rs`. Nothing here is inferred from a datasheet PDF
//! and nothing is guessed.
//!
//! # The regions, and why the splits are where they are
//!
//! ```text
//!   0x3F40_0000 +0x40_0000  DROM window   flash .rodata (cache MMU)  R
//!   0x3FF0_0000 +0x08_0000  MMIO          the peripheral window
//!   0x3FF8_0000 +0x0_2000   RTC_FAST (D)  8 KiB
//!   0x3FF9_6000 +0x0_942A   ROM data      mask ROM .rodata           R
//!   0x3FFA_E000 +0x0_2000   SRAM2 (ROM)   the 8 KiB memory.x reserves
//!   0x3FFB_0000 +0x3_0000   SRAM2 dram_seg  192 KiB app data         RW
//!   0x3FFE_0000 +0x2_0000   SRAM1 (D-bus) ROM data + both ROM stacks RW
//!   0x4000_0000 +0x6_5D90   mask ROM      code + the vector table    RX
//!   0x4007_0000 +0x3_0000   SRAM0         cache seg + vectors + IRAM RX (word-only)
//!   0x400C_0000 +0x0_2000   RTC_FAST (I)  same block as 0x3FF8_0000  RWX
//!   0x400D_0000 +0x30_0000  IROM window   flash .text (cache MMU)    RX
//!   0x5000_0000 +0x0_2000   RTC_SLOW      8 KiB                      RW
//!   0x6000_0000 +0x04_0000  MMIO (AHB)    the same peripherals, second bus (P3)
//! ```
//!
//! # Why [`SRAM1_IBUS_ALIAS_BASE`] exists as a name with no region
//!
//! The classic mirrors SRAM1 into the instruction bus at
//! `0x400A_0000..0x400C_0000` (word-swapped), and this machine **deliberately
//! does not map it**. M0's inventory
//! (`docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md` §5) measured
//! **zero** in-symbol-bounds references to that window in the shipped
//! `fw-esp32v3` image and in the classic mask ROM. Mapping a window nothing
//! reaches would only let a runaway pointer read plausible bytes.
//!
//! The name is here anyway, and that is the point: a strict-bus stop can say
//! "the SRAM1 I-bus alias, which this machine deliberately does not map"
//! instead of the far less useful "unmapped". Director ruling **DD24**
//! (`m3/notes.md` R1) confirms the omission; if a later boot does reach it,
//! the stop names it and the fix is a decision, not a discovery.
//!
//! # Why SRAM0 is one region from `0x4007_0000`
//!
//! `memory.x:13` calls `0x4007_0000 +64 KiB` `reserved_cache_seg` — memory
//! the **app** must not use, because with the cache on it *is* the cache
//! array. But the ESP-IDF second-stage bootloader executes from
//! `0x4007_8000`, inside that very segment (`m3/notes.md` §8: `esptool
//! image-info` reads bootloader segment 1 as `0x4007_8000`, 15,576 bytes, and
//! L0's ROM banner prints `load:0x40078000,len:15576`). One memory has two
//! lives, and both are real.
//!
//! So SRAM0 is **one executable region** from [`SRAM0_BASE`] to
//! [`SRAM0_END`], covering the cache segment, the 1 KiB vector table and the
//! IRAM above it. The honest diagnosis for a *cache-off* access through the
//! flash windows is D4's cache-off stop (P4), which names the DPORT bit and
//! the cycle the cache went away — not an unmapped hole that says nothing
//! about why. Director ruling **DD24**.
//!
//! # Why the SRAM0 word-only rule is a guest rule
//!
//! `lp-emu/lp-xt-emu/src/board.rs:156-172` is a **measurement**, not a
//! datasheet claim: on the desk board a byte store at `0x4008_8000` faulted
//! with `LoadStoreError` / EXCCAUSE 3 / EXCVADDR = that address, while 16,384
//! aligned word stores across `0x4008_8000..0x4009_8000` all read back.
//!
//! Guest accesses are held to that rule ([`SRAM0_WORD_ONLY`]). Host-side
//! placement is not: seeding the ROM, placing an ELF segment and filling a
//! cache line all write bytes, because they are the emulator putting memory
//! in the state silicon was handed, not the guest reaching through the
//! instruction bus. The C6's loader draws the same line for the same reason.

// ---------------------------------------------------------------------------
// The mask ROM
// ---------------------------------------------------------------------------

/// Mask ROM code, including the vector table at its base.
///
/// The vendored `esp32_rev300_rom.elf` puts `.WindowVectors.text` at
/// `0x4000_0000` and `_ResetVector` at `0x4000_0400` (`e_entry`), which is the
/// same `XCHAL_RESET_VECTOR_VADDR` `xtensa-lx-rt`'s `config/esp32.rs`
/// declares — two independent sources for one number (`m3/notes.md` §2).
pub const ROM_MASK_BASE: u32 = 0x4000_0000;

/// Through `0x4006_5D90`: the highest code byte in the ROM ELF, the end of
/// its second `.secureboot_*` chunk at `0x4006_5000 + 0xD90`
/// (`m3/notes.md` §2, from the vendored ELF's own program headers).
pub const ROM_MASK_LEN: u32 = 0x0006_5D90;

/// Mask ROM read-only data. `.rodata` has **vaddr `0x3FF9_6000`, paddr
/// `0x4006_6000`** — they differ, so a loader must place by `p_vaddr`
/// (`m3/notes.md` §2).
pub const ROM_DATA_BASE: u32 = 0x3FF9_6000;

/// Through `0x3FF9_F42A`: `.rodata` is `0x9024` bytes and a second read-only
/// chunk sits at vaddr `0x3FF9_F100 + 0x32A` (`m3/notes.md` §2).
pub const ROM_DATA_LEN: u32 = 0x0000_942A;

// ---------------------------------------------------------------------------
// SRAM0 — the instruction RAM, and the cache segment below it
// ---------------------------------------------------------------------------

/// SRAM0's base: `reserved_cache_seg` (`memory.x:13`). One region from here
/// (see the module docs) because the IDF bootloader executes from
/// `0x4007_8000`, inside it.
pub const SRAM0_BASE: u32 = 0x4007_0000;

/// `vectors_seg` (`memory.x:14`): the app's own 1 KiB vector table.
pub const SRAM0_VECTORS: u32 = 0x4008_0000;

/// `iram_seg` (`memory.x:15`), `128k - 0x400` — the app's `.rwtext`, and the
/// JIT code region at `0x4008_8000` above it.
pub const SRAM0_IRAM: u32 = 0x4008_0400;

/// The end of SRAM0. `iram_seg` runs `0x4008_0400 + (128 KiB - 0x400)`
/// (`memory.x:15`), which lands here.
pub const SRAM0_END: u32 = 0x400A_0000;

/// SRAM0's length as one region: cache segment + vectors + IRAM.
pub const SRAM0_LEN: u32 = SRAM0_END - SRAM0_BASE;

/// SRAM0 accepts **aligned word accesses only** from the guest. Measured, not
/// documented — see the module docs and `lp-xt-emu/src/board.rs:156-172`.
pub const SRAM0_WORD_ONLY: bool = true;

// ---------------------------------------------------------------------------
// SRAM1 — the ROM's data and both ROM stacks
// ---------------------------------------------------------------------------

/// SRAM1 seen from the **data** bus. `memory.x:26-35` carves the ROM's data
/// (`0x3FFE_0000 +1088`, `0x3FFE_3F20 +1072`) and both ROM stacks
/// (`0x3FFE_1320` and `0x3FFE_5230`, 11,264 bytes each) out of it, and the
/// ROM ELF's own `.stack_pro` / `.stack_app` / `.data_*` sections land on
/// exactly those addresses (`m3/notes.md` §2).
pub const SRAM1_DBUS_BASE: u32 = 0x3FFE_0000;

/// 128 KiB, up to `0x4000_0000` where the mask ROM window begins.
pub const SRAM1_DBUS_LEN: u32 = 0x0002_0000;

/// The base of the mask ROM's **PRO**-core stack: `reserved_rom_stack_pro`
/// (`memory.x:32`), which that file says is derived from the ROM's own
/// `_stack_sentry`. The vendored `esp32_rev300_rom.elf` agrees: its
/// `_stack_sentry` is `0x3FFE_1320`.
pub const ROM_PRO_STACK_BASE: u32 = 0x3FFE_1320;

/// 11,264 bytes (`memory.x:32`), the same length for both ROM stacks.
pub const ROM_STACK_LEN: u32 = 11_264;

/// The **top** of the ROM's PRO-core stack — the value the ROM's own startup
/// loads into `a1`, and the one a direct load seeds as the outermost frame's
/// stack pointer ([`crate::machine::BootFrame`]).
///
/// The ROM ELF calls it `__stack` and puts it at `0x3FFE_3F20`; `memory.x:32`
/// reaches the same address as `reserved_rom_stack_pro`'s end, and
/// `memory.x:28` starts `reserved_rom_data_app` there. Three sources, one
/// number.
pub const ROM_PRO_STACK_TOP: u32 = ROM_PRO_STACK_BASE + ROM_STACK_LEN;

/// The base of the mask ROM's **APP**-core stack (`memory.x:33`;
/// `_stack_sentry_app` in the ROM ELF).
pub const ROM_APP_STACK_BASE: u32 = 0x3FFE_5230;

/// The top of the ROM's APP-core stack: `__stack_app` in the ROM ELF,
/// `0x3FFE_7E30`, which is also `dram2_seg`'s origin (`memory.x:35`).
pub const ROM_APP_STACK_TOP: u32 = ROM_APP_STACK_BASE + ROM_STACK_LEN;

/// The SRAM1 **instruction**-bus alias, `0x400A_0000..0x400C_0000`
/// (`lp-xt-emu/src/board.rs:178-185`, the C2b measurement).
///
/// **DELIBERATELY UNMAPPED.** Named so a strict-bus stop can say which window
/// it was — see the module docs and director ruling DD24.
pub const SRAM1_IBUS_ALIAS_BASE: u32 = 0x400A_0000;

/// The alias's length, for the same naming purpose. It ends where RTC fast
/// memory's I-bus view begins ([`RTC_FAST_IBUS`]).
pub const SRAM1_IBUS_ALIAS_LEN: u32 = 0x0002_0000;

// ---------------------------------------------------------------------------
// SRAM2 — the app's data RAM
// ---------------------------------------------------------------------------

/// The 8 KiB below `dram_seg` that `memory.x:19` reserves for the ROM
/// (`ORIGIN = 0x3FFAE000 + 8K`). The ROM's non-alloc `.data_c`,
/// `.data_phyrom`, `.data_spi_flash` and `.data_btdm` sections live in it
/// (`m3/notes.md` §2), which is why it is a region and not a hole.
pub const SRAM2_ROM_RESERVED: u32 = 0x3FFA_E000;

/// The reserve `memory.x:19` names `RESERVE_DRAM`. `third_party/esp-hal`'s
/// `build.rs:262-266` makes it `0x10000` under `__bluetooth` and `0x0`
/// otherwise; `fw-esp32v3` enables no Bluetooth feature, so it is zero and
/// `dram_seg` starts exactly 8 KiB above [`SRAM2_ROM_RESERVED`].
pub const RESERVE_DRAM: u32 = 0;

/// `dram_seg` (`memory.x:19`): `0x3FFA_E000 + 8K + RESERVE_DRAM`.
///
/// **Confirmed from the other side by L0** — the desk board's IDF bootloader
/// mapped the app's DRAM segments to `vaddr=3ffb0000` and `vaddr=3ffb0010`
/// (`m3/notes.md` §3).
pub const DRAM_SEG_BASE: u32 = SRAM2_ROM_RESERVED + 8 * 1024 + RESERVE_DRAM;

/// `192K - RESERVE_DRAM` (`memory.x:19`), so 192 KiB on this image.
pub const DRAM_SEG_LEN: u32 = 0x0003_0000 - RESERVE_DRAM;

// ---------------------------------------------------------------------------
// The flash windows
// ---------------------------------------------------------------------------

/// The IROM window: the app's `.text`, mapped through the cache MMU.
///
/// `memory.x:46` links at `0x400D_0020 + (3M - 0x20)`; the `0x20` is the
/// image-header convenience that satisfies the MMU's `paddr % 64K == vaddr %
/// 64K`, not a hardware boundary, so the region starts at the window base and
/// the first 32 bytes are addressable rather than a hole. L0's bootloader
/// mapped the app's IROM segment to `vaddr=400d0020`.
pub const IROM_BASE: u32 = 0x400D_0000;

/// 3 MiB (`memory.x:46`).
pub const IROM_LEN: u32 = 0x0030_0000;

/// The DROM window: the app's `.rodata`. `memory.x:47`, `0x3F40_0020 + (4M -
/// 0x20)`; same `0x20` reasoning as [`IROM_BASE`].
pub const DROM_BASE: u32 = 0x3F40_0000;

/// 4 MiB (`memory.x:47`).
pub const DROM_LEN: u32 = 0x0040_0000;

// ---------------------------------------------------------------------------
// RTC memory
// ---------------------------------------------------------------------------

/// `rtc_fast_iram_seg` (`memory.x:51`), 8 KiB, executable, PRO core only.
pub const RTC_FAST_IBUS: u32 = 0x400C_0000;

/// `rtc_fast_dram_seg` (`memory.x:54`) — **the same 8 KiB block** seen from
/// the data bus. Two addresses, one memory: this is the view the machine
/// maps, and [`RTC_FAST_IBUS`] is named and unmapped (DD36; see
/// [`RAM_SPANS`]), because two stores would have two answers for one byte.
pub const RTC_FAST_DBUS: u32 = 0x3FF8_0000;

/// 8 KiB (`memory.x:51`/`:54`).
pub const RTC_FAST_LEN: u32 = 0x0000_2000;

/// `rtc_slow_seg` (`memory.x:57`), 8 KiB, data only. The firmware's recovery
/// ledger lives here.
pub const RTC_SLOW_BASE: u32 = 0x5000_0000;

/// 8 KiB (`memory.x:57`).
pub const RTC_SLOW_LEN: u32 = 0x0000_2000;

// ---------------------------------------------------------------------------
// MMIO
// ---------------------------------------------------------------------------

/// The peripheral window on the **DPORT bus**. Every base in [`periph`]
/// except the two AHB-only ones falls inside `0x3FF0_0000..0x3FF8_0000` —
/// the window is declared to `0x3FF8_0000` because that is where RTC fast
/// memory's data view begins ([`RTC_FAST_DBUS`]).
pub const MMIO_BASE: u32 = 0x3FF0_0000;

/// `0x8_0000`, ending at [`RTC_FAST_DBUS`].
pub const MMIO_LEN: u32 = 0x0008_0000;

/// The peripheral window on the **AHB bus** — the classic's second
/// peripheral window, found by P3's fifth strict stop on the direct load.
///
/// The mask ROM's `rom_chip_i2c_writeReg` (`0x4000_4168`) computes the
/// analog I2C master's command word address as
/// `(0x1800_3800 + host_id) << 2` = **`0x6000_E000 + 4·host_id`**, and
/// `esp_hal::soc::esp32::clocks` reaches it through `rom_i2c_writeReg` to
/// program the BBPLL (`REGI2C_BBPLL(0x66, 4)` → `0x6000_E010`, the address
/// of the stop). The address is outside the DPORT window and inside nothing
/// this file named until then.
///
/// **It is a mirror.** The ROM's `.text` loads forty-odd literals in
/// `0x6000_0000..0x6002_2000`, and they line up with DPORT blocks the PAC
/// names, offset by `0x3FF4_0000 − 0x6000_0000`: `0x6000_88xx` is `SENS`
/// (`0x3FF4_8800`), `0x6001_C0xx`/`0x6001_D0xx` are `NRX`/`BB`
/// (`0x3FF5_CC00`/`0x3FF5_D000`), `0x6000_60xx` is `FLASH_ENCRYPTION`
/// (`0x3FF4_6000`). And the PAC's `RNG` at `0x6003_5000` — which P1 excluded
/// as "an address that does not exist on this part" — is the AHB address of
/// the WiFi window's `WDEV` block: its `data` register at `+0x144` is
/// `0x6003_5144`, the classic's `WDEV_RND_REG`, which esp-hal's classic
/// `rng` reads through that very PAC type. **P1's exclusion was wrong**, and
/// P3 reverses it (`regs::RNG`, and the generator's `SKIP` entry removed).
///
/// So: `AHB = 0x6000_0000 + (DPORT − 0x3FF4_0000)` for the blocks from
/// `UART0` up, `0x4_0000` long, ending where the DPORT window ends.
///
/// **What this machine does with the mirror, since P4 (DD38).** P3 could
/// register a block at only *one* of its two addresses — two registrations
/// would be two blocks with two states, which is the SRAM1-alias mistake in
/// MMIO form (DD24/DD36) — and reported the question. The director ruled
/// **DD38** and P4 landed `SocBus::add_peripheral_alias`: **one state, two
/// decodes**. Every block this machine registers inside `0x3FF4_0000..` now
/// also answers at [`dport_to_ahb`]'s address, and the trace flags an access
/// that came through the alias so the two doors stay distinguishable.
///
/// ⚠️ The alias is registered **only in the direction the evidence runs**:
/// from a PAC-named DPORT base to its AHB address. [`periph::I2C_ANA_MST`],
/// whose only cited address is the AHB one (the PAC names no block there),
/// gets **no** DPORT-side alias — putting one at `0x3FF4_E000` would be a
/// claim about a base nothing in this repo names, and a guest reaching it
/// should stop rather than silently find the analog master.
///
/// The blocks the ROM's `main` reaches through AHB and this machine does not
/// map at all (`SENS`, `FE`/`FE2`, `NRX`/`BB`, `FLASH_ENCRYPTION`) stay
/// unmapped: an alias needs a registered block to point at. They are P5's and
/// P7's, and their AHB addresses become aliases when they are registered.
pub const MMIO_AHB_BASE: u32 = 0x6000_0000;

/// `0x4_0000`: the mirror of `0x3FF4_0000..0x3FF8_0000`. See
/// [`MMIO_AHB_BASE`].
pub const MMIO_AHB_LEN: u32 = 0x0004_0000;

/// The low end of the DPORT window the AHB bus mirrors. Below it — `DPORT`
/// itself at `0x3FF0_0000`, `AES`, `RSA`, `SHA` — there is no AHB address,
/// which is why the ROM reaches those four only through the DPORT bus.
pub const MMIO_AHB_MIRROR_FROM: u32 = 0x3FF4_0000;

/// The DPORT address a mirrored AHB address corresponds to, or `None` for an
/// address outside the AHB window. Reporting only: the forwarding itself is
/// the bus's ([`dport_to_ahb`] is what registers it).
pub const fn ahb_to_dport(address: u32) -> Option<u32> {
    if address >= MMIO_AHB_BASE && address < MMIO_AHB_BASE + MMIO_AHB_LEN {
        Some(address - MMIO_AHB_BASE + MMIO_AHB_MIRROR_FROM)
    } else {
        None
    }
}

/// The AHB address that mirrors a DPORT one, or `None` for a DPORT address
/// the AHB window does not cover. [`ahb_to_dport`]'s inverse, and the
/// function [`crate::machine::Machine`] registers a block's alias from
/// (DD38).
pub const fn dport_to_ahb(address: u32) -> Option<u32> {
    if address >= MMIO_AHB_MIRROR_FROM && address < MMIO_AHB_MIRROR_FROM + MMIO_AHB_LEN {
        Some(address - MMIO_AHB_MIRROR_FROM + MMIO_AHB_BASE)
    } else {
        None
    }
}

/// The flash MMU page tables, **PRO** core: 256 `u32` entries of 64 KiB pages
/// at `0x3FF1_0000`.
///
/// ⚠️ These are **not** in the PAC's DPORT register block — they are raw
/// arrays past the end of what svd2rust generates, so `pac-regnames.py`
/// cannot name them (`m3/notes.md` §4). P4 declares their format from the
/// ROM's own `Cache_Flash_MMU_Set`, never from a datasheet. Named here so the
/// address itself has one home.
pub const FLASH_MMU_PRO: u32 = 0x3FF1_0000;

/// The flash MMU page tables, **APP** core, at `0x3FF1_2000`. See
/// [`FLASH_MMU_PRO`].
pub const FLASH_MMU_APP: u32 = 0x3FF1_2000;

/// Peripheral base addresses, from `esp32-0.40.2/src/lib.rs` (every
/// peripheral is `Periph<RegisterBlock, BASE>`; the line number is on each).
/// The window each block gets is decided where it is registered — P2 onward.
///
/// Two bases are on the **AHB** bus ([`MMIO_AHB_BASE`]), not the DPORT one:
/// [`periph::I2C_ANA_MST`], which the PAC does not name at all, and
/// [`periph::RNG`], which P1 wrongly excluded as an SVD leak — see
/// [`MMIO_AHB_BASE`] for the evidence that reversed it.
pub mod periph {
    /// `:458`. The system/DPORT block: cache control, the per-core interrupt
    /// maps, the peripheral clock and reset gates.
    pub const DPORT: u32 = 0x3FF0_0000;
    /// `:431`.
    pub const AES: u32 = 0x3FF0_1000;
    /// `:656`.
    pub const RSA: u32 = 0x3FF0_2000;
    /// `:710`. The IDF bootloader hashes the app image with it (P7).
    pub const SHA: u32 = 0x3FF0_3000;
    /// `:800`. The console: the ROM banner at 115200 and the app at 921600.
    pub const UART0: u32 = 0x3FF4_0000;
    /// `:746`. SPI1 — the flash controller esp-storage drives.
    pub const SPI1: u32 = 0x3FF4_2000;
    /// `:737`. SPI0 — the cache's own flash port. The PAC gives SPI0 and SPI1
    /// the same `spi0::RegisterBlock`, which is why one generated table
    /// serves both.
    pub const SPI0: u32 = 0x3FF4_3000;
    /// `:521`.
    pub const GPIO: u32 = 0x3FF4_4000;
    /// `:503`.
    pub const FLASH_ENCRYPTION: u32 = 0x3FF4_6000;
    /// `:512`.
    pub const FRC_TIMER: u32 = 0x3FF4_7000;
    /// `:665`. Reset cause, the CPU stall keys, and the RWDT.
    pub const RTC_CNTL: u32 = 0x3FF4_8000;
    /// `:674`.
    pub const RTC_IO: u32 = 0x3FF4_8400;
    /// `:701`.
    pub const SENS: u32 = 0x3FF4_8800;
    /// `:683`.
    pub const RTC_I2C: u32 = 0x3FF4_8C00;
    /// `:584`.
    pub const IO_MUX: u32 = 0x3FF4_9000;
    /// `:809`.
    /// I2S0 — ⚠️ on the boot path as the **RNG's entropy source**, not as
    /// an audio block: the ESP-IDF second-stage bootloader's
    /// `bootloader_random_enable()` drives its ADC-sampling mode to stir the
    /// hardware RNG (`esp32-0.40.2/src/lib.rs`, `Periph<i2s0::RegisterBlock,
    /// 0x3ff4_f000>`).
    pub const I2S0: u32 = 0x3FF4_F000;
    pub const UART1: u32 = 0x3FF5_0000;
    /// `:638`. The block's pulse RAM is at `0x3FF5_6800` (M4).
    pub const RMT: u32 = 0x3FF5_6000;
    /// `:638` + `0x800`: RMT's own 512-word pulse memory.
    pub const RMT_RAM: u32 = 0x3FF5_6800;
    /// `:467`. MAC and chip revision.
    pub const EFUSE: u32 = 0x3FF5_A000;
    /// `:773`. The esp-rtos tick, and LACT as the classic's clock (P5).
    pub const TIMG0: u32 = 0x3FF5_F000;
    /// `:782`. Shares `timg0::RegisterBlock` with [`TIMG0`].
    pub const TIMG1: u32 = 0x3FF6_0000;
    /// `:440`.
    pub const APB_CTRL: u32 = 0x3FF6_6000;
    /// `:818`.
    pub const UART2: u32 = 0x3FF6_E000;
    /// `:836`. The WiFi MAC window. Not mapped: the shipped image runs no
    /// radio.
    pub const WIFI: u32 = 0x3FF7_3000;
    /// **AHB bus.** The analog I2C master the mask ROM's `rom_i2c_writeReg`
    /// / `rom_i2c_readReg` drive (`0x4000_41A4` / `0x4000_4148`, through
    /// `rom_chip_i2c_writeReg` at `0x4000_4168`: address `(0x1800_3800 +
    /// host_id) << 2`). One command/status word per `host_id`; the BBPLL is
    /// host 4. The PAC has no block for it; the ROM is the citation.
    pub const I2C_ANA_MST: u32 = 0x6000_E000;
    /// `:647`, **AHB bus**: `0x6003_5000`, with `data` at `+0x144` =
    /// `0x6003_5144`, the classic's `WDEV_RND_REG`. The DPORT-side twin is
    /// the WiFi window ([`WIFI`] + `0x2000`); the PAC names the AHB one.
    pub const RNG: u32 = 0x6003_5000;
}

/// CPU clock: 240 MHz. `CpuClock::max()` on this chip
/// (`lp-fw/fw-esp32v3/src/board/esp32v3/init.rs:49-53`, which says so and
/// sets it). Guest microseconds are `cycles / 240`.
pub const CPU_HZ: u64 = 240_000_000;

/// Cycles per emulated microsecond.
pub const CYCLES_PER_US: u64 = CPU_HZ / 1_000_000;

/// One named span of the map, for the machine's `--map` style reporting and
/// for the tests that assert the map does not drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub name: &'static str,
    pub base: u32,
    pub len: u32,
}

impl Span {
    pub const fn end(&self) -> u32 {
        self.base + self.len
    }

    pub const fn contains(&self, address: u32) -> bool {
        address >= self.base && address < self.base + self.len
    }
}

/// The RAM-backed regions, in address order. What P2's machine builder
/// registers on the bus.
///
/// `rtc-fast-dbus` and `rtc-fast-ibus` are two spans of **one** 8 KiB memory
/// (`memory.x:51` and `:54` say so in as many words); they are listed
/// separately because they decode separately. ⚠️ **Only the D-bus span is
/// mapped** (ruling DD36, amending what this comment promised before P3):
/// `SocBus` keeps one flat store per region and cannot put one store behind
/// two windows, so `bus_setup` maps `rtc-fast-dbus` and leaves
/// `rtc-fast-ibus` named and unmapped, the way the SRAM1 I-bus alias is. A
/// strict stop at `0x400C_xxxx` is the evidence that would justify an alias
/// region and a phase of its own.
pub const RAM_SPANS: &[Span] = &[
    Span {
        name: "drom-window",
        base: DROM_BASE,
        len: DROM_LEN,
    },
    Span {
        name: "rtc-fast-dbus",
        base: RTC_FAST_DBUS,
        len: RTC_FAST_LEN,
    },
    Span {
        name: "rom-data",
        base: ROM_DATA_BASE,
        len: ROM_DATA_LEN,
    },
    Span {
        name: "sram2-rom-reserved",
        base: SRAM2_ROM_RESERVED,
        len: 8 * 1024,
    },
    Span {
        name: "dram-seg",
        base: DRAM_SEG_BASE,
        len: DRAM_SEG_LEN,
    },
    Span {
        name: "sram1-dbus",
        base: SRAM1_DBUS_BASE,
        len: SRAM1_DBUS_LEN,
    },
    Span {
        name: "rom-mask",
        base: ROM_MASK_BASE,
        len: ROM_MASK_LEN,
    },
    Span {
        name: "sram0",
        base: SRAM0_BASE,
        len: SRAM0_LEN,
    },
    Span {
        name: "rtc-fast-ibus",
        base: RTC_FAST_IBUS,
        len: RTC_FAST_LEN,
    },
    Span {
        name: "irom-window",
        base: IROM_BASE,
        len: IROM_LEN,
    },
    Span {
        name: "rtc-slow",
        base: RTC_SLOW_BASE,
        len: RTC_SLOW_LEN,
    },
];

/// The declared MMIO windows. An access inside one that no peripheral claims
/// is still unmapped, but the log says "an unmodelled block".
///
/// Two since P3: the DPORT window and its AHB mirror ([`MMIO_AHB_BASE`]).
pub const MMIO_WINDOWS: &[Span] = &[
    Span {
        name: "mmio-dport",
        base: MMIO_BASE,
        len: MMIO_LEN,
    },
    Span {
        name: "mmio-ahb",
        base: MMIO_AHB_BASE,
        len: MMIO_AHB_LEN,
    },
];

/// Windows this machine names and deliberately does **not** map. A strict
/// stop inside one of these says which window it was, instead of "unmapped".
///
/// Today that is the SRAM1 I-bus alias alone (DD24; see the module docs).
pub const UNMAPPED_BY_DESIGN: &[Span] = &[Span {
    name: "sram1-ibus-alias",
    base: SRAM1_IBUS_ALIAS_BASE,
    len: SRAM1_IBUS_ALIAS_LEN,
}];
