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
/// the data bus. Two addresses, one memory; the machine backs them with one
/// store or it will have two answers for one byte.
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

/// The peripheral window. Every base in [`periph`] falls inside
/// `0x3FF0_0000..0x3FF8_0000` — the window is declared to `0x3FF8_0000`
/// because that is where RTC fast memory's data view begins
/// ([`RTC_FAST_DBUS`]).
pub const MMIO_BASE: u32 = 0x3FF0_0000;

/// `0x8_0000`, ending at [`RTC_FAST_DBUS`].
pub const MMIO_LEN: u32 = 0x0008_0000;

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
/// ⚠️ **`RNG` is not here.** The PAC's `:647` gives it `0x6003_5000`, an
/// address that does not exist on the classic (an SVD leak from the S2/C3
/// family). The classic's random register is `WDEV_RND_REG` inside the WiFi
/// window; P7 resolves it from the ROM ELF's own symbol or from esp-hal's
/// classic `rng` — never from that PAC line. `pac-regnames.py` carries the
/// same exclusion in its `SKIP` table.
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
    /// radio, and the classic's `WDEV_RND_REG` lives in here (P7).
    pub const WIFI: u32 = 0x3FF7_3000;
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
/// separately because they decode separately, and P2 backs them with one
/// store.
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
pub const MMIO_WINDOWS: &[Span] = &[Span {
    name: "mmio",
    base: MMIO_BASE,
    len: MMIO_LEN,
}];

/// Windows this machine names and deliberately does **not** map. A strict
/// stop inside one of these says which window it was, instead of "unmapped".
///
/// Today that is the SRAM1 I-bus alias alone (DD24; see the module docs).
pub const UNMAPPED_BY_DESIGN: &[Span] = &[Span {
    name: "sram1-ibus-alias",
    base: SRAM1_IBUS_ALIAS_BASE,
    len: SRAM1_IBUS_ALIAS_LEN,
}];
