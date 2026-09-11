//! Direct load — start at the application's entry with the machine in the
//! state the mask ROM and the ESP-IDF second-stage bootloader would have
//! left it in.
//!
//! The reason this file is not simply "put the ELF in memory and jump" is
//! the vision's line: **the bootloader matters for memory.** On the classic
//! it is also the reason the hart's `PS` and `a1` are not the architectural
//! reset values: the bootloader reaches the application through an ordinary
//! C call, which on the windowed ABI is `callx8`, so the app's `Reset:` runs
//! as frame 2 on a stack the ROM set up and the bootloader has partly used.
//! [`BOOTLOADER_SP_AT_APP_ENTRY`] is that stack pointer, pinned from the
//! disassembly, and [`crate::machine::BootFrame::idf_bootloader`] is the
//! direct load's default frame because of it.
//!
//! # What this reproduces
//!
//! - Every `PT_LOAD` at its **`vaddr`**, with the `memsz - filesz` tail
//!   zeroed ([`load_app`]). The shipped `fw-esp32v3` image has **one**
//!   segment whose `paddr` differs from its `vaddr`: `.rtc_fast.persistent`
//!   links to RTC fast memory at `0x3FF8_0000` with a load address in the
//!   DROM window (`readelf -l`: `0x3ff80000` / `0x3f447410`, `filesz 0`,
//!   `memsz 0x3d0`). It is NOBITS — the bytes are all zero either way — but
//!   the loader records the mismatch ([`PlacedAppSegment::relocated`])
//!   instead of quietly picking one, and `tests/boot.rs` asserts it is that
//!   segment and no other.
//! - **`.data` placed.** ⚠️ The classic differs from the C6 *in mechanism and
//!   not in outcome*. On the C6 `hal-defaults.x:53-56` hardcodes
//!   `__sdata = __edata = __sidata = 0` so `_start`'s copy loop is a no-op.
//!   On the classic `third_party/esp-hal/ld/xtensa/hal-defaults.x:3-4`
//!   `PROVIDE`s `__init_data` as `default_mem_hook`, which
//!   `xtensa-lx-rt-0.22.0/src/lib.rs:225-227` returns **`true`** from — so
//!   the app's `Reset` (`lib.rs:128-160`) **does** run `_xtensa_lx_rt_copy`
//!   over `_sidata → _data_start.._data_end`. But `ld/sections/rwdata.x`
//!   places `.data` `> RWDATA` with **no `AT>`** and `build.rs:114` aliases
//!   `RWDATA = dram_seg`, so LMA == VMA and the copy is a **self-copy**:
//!   each word is moved onto itself. **Verified on the shipped image** with
//!   `xtensa-esp32-elf-nm`: `_sidata = 0x3ffb0000` and `_data_start =
//!   0x3ffb0000` — the same address, as the design requires. So the bytes
//!   must already be at `0x3FFB_0000` before `Reset` runs, which is what
//!   placing the DRAM `PT_LOAD` at its vaddr does. A mis-placed `.data`
//!   cannot be repaired by the guest.
//! - **`.bss` zeroed by the app**, not the loader: `Reset` calls
//!   `_xtensa_lx_rt_zero_fill` (`lib.rs:139-146`). The loader zeroes it
//!   anyway, because a `PT_LOAD`'s tail past `filesz` is zeroed, and the two
//!   agree.
//! - **The mask ROM loaded first**, then the app's segments over it — the
//!   order the real bootloader runs in ([`crate::machine::Esp32V3Builder`]
//!   places the ROM and seeds its non-alloc data before calling here).
//! - Hart state: `pc = e_entry`, GPRs zero except `a1`, `PS = PS_BOOT =
//!   0x0006_0020` (`WOE | UM | CALLINC(2)` — the `callx8`), `VECBASE =
//!   0x4000_0000` (the ROM's; the app repoints it itself at `lib.rs:185-188`),
//!   `CCOUNT`/`CCOMPARE0..2 = 0` (architecturally undefined at reset; the
//!   runtime zeroes the compares anyway, `lib.rs:165-167`), **`CPENABLE =
//!   0`** — the firmware arms it itself (`board/esp32v3/init.rs:64`,
//!   `super::fpu::arm()`), and seeding it would hide an `EXCCAUSE=32` the
//!   guest is entitled to take. All of those are what `XtHart::new` leaves
//!   and what [`crate::machine::Machine::seed_boot_state`] sets; nothing here
//!   writes a special register.
//! - `dram_seg` as plain zeroed RAM, and the ROM's two stacks
//!   (`0x3FFE_1320 + 0x2C00`, `0x3FFE_5230 + 0x2C00`) as plain RAM: the
//!   firmware reclaims both as heap regions 0 and 3 (L0's `[INIT] heap
//!   regions:` line, `m3/notes.md` §11).
//! - **The flash chip's `chip_size`** written into the ROM's chip
//!   description ([`seed_rom_flash_chip`]) in place of the bootloader's
//!   `esp_rom_spiflash_config_param`. See that function for the symbol,
//!   which is **not** the one `m3/notes.md` §3 names.
//!
//! # What this does NOT reproduce — the direct-load ↔ ROM-up cross-check's seed
//!
//! Written down here because P7 boots the same image from the reset vector
//! through the real ROM and the real IDF bootloader, and every line below is
//! a place the two paths can disagree (`m3/notes.md` §3, the eleven):
//!
//! 1. **No partition table** is read or validated; no `esp_app_desc`,
//!    image-hash, secure-boot or flash-encryption path. A corrupt image boots
//!    here and does not there.
//! 2. **The flash MMU page table is programmed by the loader, not by a
//!    bootloader** — and in P3 it is not programmed at all: the IROM/DROM
//!    windows are plain RAM regions holding the ELF's bytes, with no flash
//!    chip behind them. P7 stages the image in the chip and maps the pages
//!    the way the C6's `stage_image_in_flash` does; the flash offsets will be
//!    the loader's arithmetic, not an `esptool` image's segment layout.
//! 3. **The ROM console is never initialised.** `uartAttach`,
//!    `ets_install_uart_printf` and the printf-channel globals are untouched,
//!    so `ets_printf` from the app answers from an unwritten ROM data
//!    segment.
//! 4. **No ROM banner** (`ets Jul 29 2019 12:21:46`, `rst:0x1
//!    (POWERON_RESET)…`) **and no bootloader log** — a large part of what a
//!    boot transcript compares, and none of it exists on this path.
//! 5. **No early RNG entropy** (the bootloader's *"Enabling RNG early entropy
//!    source"* step); the machine's seeded PRNG stands in, deterministic by
//!    design and therefore not what silicon had.
//! 6. **eFuse is asserted, not read.** The MAC and the chip revision come
//!    from whatever the EFUSE block is seeded with, not from a chip.
//! 7. **The reset cause is asserted as POWERON** ([`ResetCause`]). Nothing
//!    consulted `RTC_CNTL.reset_state` to decide it.
//! 8. **`.data` is placed rather than copied** — the app's own self-copy
//!    runs and is a no-op (above), so a mis-placed `.data` cannot be
//!    repaired by the guest.
//! 9. **Core 1 is never released from `sw_stall`** — a direct load starts
//!    with the PRO core running and the APP core stalled by assertion
//!    (`Machine::stalled[1]`), where silicon has it stalled by the ROM's own
//!    code path through `RTC_CNTL.options0` / `sw_cpu_stall` and
//!    `DPORT.appcpu_ctrl_c`.
//! 10. **The flash chip's `chip_size`** is written by the loader
//!     ([`seed_rom_flash_chip`]) in place of the bootloader's
//!     `esp_rom_spiflash_config_param`. Default 4 MiB, the desk board (L0).
//! 11. **The cache MMU is left in the enabled state the bootloader hands
//!     over** — [`seed_cache_enabled`], since P4. The PRO core's
//!     `pro_cache_enable` is **1** at the application's entry because the IDF
//!     bootloader calls `Cache_Read_Enable` immediately before the `callx8`
//!     that enters the app (the P3 report's §2 item 4 pins the app's entry to
//!     `0x400796b9`, "between the `Cache_Read_Enable` call and the next
//!     function's `entry`"), and the ROM's own `ets_unpack_flash_code` calls
//!     it too (`400070b3: call8 <Cache_Read_Enable>`). A direct load runs
//!     neither, so without the seed D4's stop fires eight instructions in on
//!     the app's own `__pre_init` — which is the emulator being right about
//!     its own state and wrong about silicon's. The **page table** behind the
//!     window is still not programmed (item 2): the flash windows are plain
//!     RAM holding the ELF's bytes until P7 stages an image and maps it.
//!
//! # The stack pointer at the application's entry, pinned
//!
//! P2 seeded `a1` = the mask ROM's `__stack` (`0x3FFE_3F20`) and said the
//! bootloader's own SP at the `callx8` into the app was P3's to pin. It is
//! pinned here from the disassembly of both halves of the chain, and the
//! chain is [`BOOTLOADER_FRAME_CHAIN`]:
//!
//! ```text
//! mask ROM  _start            0x40000706  l32r a1, __stack      a1 = 0x3FFE_3F20
//!           0x4000073d        call4 main
//! mask ROM  main              0x400076c4  entry a1, 112         a1 = 0x3FFE_3EB0
//!           0x40007c15        callx8 [user_code_start]   (0x3FFE_0400, the bootloader's entry)
//! IDF bl    call_start_cpu0   0x4008064c  entry a1, 192         a1 = 0x3FFE_3DF0
//!           0x400806b5        call8 0x40079a5c
//! IDF bl    …load_boot_image  0x40079a5c  entry a1, 304         a1 = 0x3FFE_3CC0
//!           0x40079a81        call8 0x400795a8
//! IDF bl    load_image        0x400795a8  entry a1, 64          a1 = 0x3FFE_3C80
//!           0x400796b9        callx8 a2                  (the app's e_entry)
//! ```
//!
//! So the application's `Reset` is entered with **`a1 = 0x3FFE_3C80`** and
//! `PS.CALLINC = 2`. The bootloader never sets a stack pointer of its own —
//! the whole IDF bootloader binary contains no `movi`/`l32r`/`movsp` into
//! `a1` — it runs on the ROM's PRO stack, 672 bytes down. The ROM half is
//! read from the vendored `esp32_rev300_rom.elf` with symbols; the
//! bootloader half from the bootloader `espflash save-image --chip esp32
//! --merge` embeds (ESP-IDF `v5.1-beta1-378-gea5e0ff298`, entry
//! `0x4008064c`, `esptool image-info` segment 1 at `0x4007_8000` and segment
//! 3 at `0x4008_0404`), disassembled raw with `xtensa-esp32-elf-objdump -D
//! -b binary -m xtensa --adjust-vma`. The IDF names are from the source
//! (`bootloader_start.c: call_start_cpu0`, `bootloader_utility.c:
//! bootloader_utility_load_boot_image` → `load_image` → the inlined
//! `unpack_load_app`/`set_cache_and_start_app`, whose `(*entry)()` carries
//! the comment *"we have used quite a bit of stack at this point"*); the
//! addresses are what the binary says, and the `callx8 a2` at `0x400796b9`
//! sits between the `Cache_Read_Enable` call and the next function's
//! `entry`, which is what a `noreturn` call into the app looks like.
//!
//! What the four words of the base save area at `[a1-16, a1)` hold on
//! silicon — the spilled `a0..a3` of `load_image`'s caller — is **not**
//! pinned here; `BootFrame::at` seeds `[0, a1, 0, 0]`, and P7's ROM-up run,
//! which arrives at the same `callx8` with the real values in the register
//! file, is the measurement that either confirms or replaces them.

use lp_emu_esp_common::{ElfImage, SocBus};

use crate::cache::{CacheHandle, FlashMmu};
use crate::flash::{FACTORY_LEN, FACTORY_OFFSET, FlashHandle};
use crate::machine::BootFrame;
use crate::memmap;
use crate::rom::{self, RomError, place_spanning};

/// The flash chip a run assumes when nothing says otherwise: **4 MiB**, the
/// desk board (DOM-Z-102, `../bench.md`: "4 MB flash"), and the size
/// `justfile`'s `v3_flash_size` flashes with.
pub const DEFAULT_FLASH_SIZE: u32 = 4 * 1024 * 1024;

/// The ROM's own name for its SPI flash chip description. See
/// [`seed_rom_flash_chip`].
pub const ROM_FLASH_CHIP_SYMBOL: &str = "spi_w25q16";

/// The byte offset of `chip_size` inside the ROM's chip description: the
/// second word of `esp_rom_spiflash_chip_t` (`device_id, chip_size,
/// block_size, sector_size, page_size, status_mask`).
pub const ROM_FLASH_CHIP_SIZE_OFFSET: u32 = 4;

/// The ROM's default `chip_size`: `0x0020_0000`, **2 MiB**, read out of
/// `.data_spi_flash` in the vendored ELF (`objdump -s -j .data_spi_flash`:
/// `ef401500 00002000 …`). The `lpfs` partition starts at `0x0031_0000`,
/// past it, which is why the seed exists.
pub const ROM_DEFAULT_FLASH_SIZE: u32 = 0x0020_0000;

/// The stack pointer the IDF bootloader hands the application: the mask
/// ROM's `__stack` minus the four frames between `_start` and the `callx8`
/// into the app. See the module docs for the derivation and
/// [`BOOTLOADER_FRAME_CHAIN`] for the frames.
pub const BOOTLOADER_SP_AT_APP_ENTRY: u32 = 0x3FFE_3C80;

/// One frame between the ROM's `__stack` and the app's entry:
/// `(who, the pc of its `entry`, the bytes that `entry` reserves)`.
///
/// `tests/boot.rs` re-derives [`BOOTLOADER_SP_AT_APP_ENTRY`] from this table
/// and [`memmap::ROM_PRO_STACK_TOP`], so the constant and its derivation
/// cannot drift apart.
pub const BOOTLOADER_FRAME_CHAIN: &[(&str, u32, u32)] = &[
    ("mask ROM main", 0x4000_76C4, 112),
    ("IDF bootloader call_start_cpu0", 0x4008_064C, 192),
    (
        "IDF bootloader bootloader_utility_load_boot_image",
        0x4007_9A5C,
        304,
    ),
    ("IDF bootloader load_image", 0x4007_95A8, 64),
];

impl BootFrame {
    /// The frame the IDF bootloader leaves at the application's entry:
    /// `a1 = ` [`BOOTLOADER_SP_AT_APP_ENTRY`], with the default save area.
    /// The direct load's default from P3 on.
    pub const fn idf_bootloader() -> Self {
        Self::at(BOOTLOADER_SP_AT_APP_ENTRY)
    }
}

/// The desk board's MAC — `30:76:f5:ec:f6:34`, the classic ESP32 on
/// `../bench.md`'s bench (DOM-Z-102). The default so that a run with no
/// `--efuse-*` flag produces the same identity as the board every transcript
/// came from, exactly as the C6's `DESK_MAC` does.
pub const DESK_MAC: [u8; 6] = [0x30, 0x76, 0xf5, 0xec, 0xf6, 0x34];

/// What the classic's eFuse block reports about the part it is: the MAC and
/// the chip revision. [`crate::periph::efuse`] turns it into the words
/// esp-hal and the IDF bootloader read.
///
/// The desk board is **v3.1** — L0's bootloader banner prints `chip
/// revision: v3.1` (`../bench.md`) — so that is the default.
///
/// ⚠️ **The major revision is not an eFuse field alone.** esp-hal's
/// `major_chip_version` (`esp-hal-1.1.1/src/efuse/esp32/mod.rs`) combines
/// three bits: `CHIP_VER_REV1` (eFuse block 0 bit 111), `CHIP_VER_REV2`
/// (bit 180) and **bit 31 of `APB_CTRL.date`**, which is not an eFuse at
/// all. See [`crate::periph::accept::apb_ctrl`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EfuseIdentity {
    pub mac: [u8; 6],
    /// Major chip revision — 3 on the desk board. Only 0..=3 are
    /// expressible: esp-hal maps the three-bit combination to exactly those
    /// four values.
    pub chip_major: u8,
    /// Minor chip revision — 1 on the desk board. Two bits
    /// (`WAFER_VERSION_MINOR`, block 0 bits 184:185).
    pub chip_minor: u8,
}

impl Default for EfuseIdentity {
    fn default() -> Self {
        Self {
            mac: DESK_MAC,
            chip_major: 3,
            chip_minor: 1,
        }
    }
}

impl EfuseIdentity {
    /// Parse `30:76:f5:ec:f6:34`.
    pub fn parse_mac(text: &str) -> Result<[u8; 6], String> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 6 {
            return Err(format!("`{text}` is not six colon-separated octets"));
        }
        let mut mac = [0u8; 6];
        for (i, p) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(p, 16).map_err(|e| format!("`{p}`: {e}"))?;
        }
        Ok(mac)
    }

    /// Parse `3.1` into (major, minor).
    pub fn parse_rev(text: &str) -> Result<(u8, u8), String> {
        let (major, minor) = text
            .split_once('.')
            .ok_or_else(|| format!("`{text}` is not `<major>.<minor>`"))?;
        let major: u8 = major.parse().map_err(|e| format!("`{major}`: {e}"))?;
        let minor: u8 = minor.parse().map_err(|e| format!("`{minor}`: {e}"))?;
        if major > 3 {
            return Err(format!(
                "chip revision major {major}: esp-hal's three-bit combination expresses 0..=3 \
                 only (efuse/esp32/mod.rs `major_chip_version`)"
            ));
        }
        if minor > 3 {
            return Err(format!(
                "chip revision minor {minor}: WAFER_VERSION_MINOR is two bits (block 0 bits \
                 184:185)"
            ));
        }
        Ok((major, minor))
    }
}

/// Why the chip is starting, as `RTC_CNTL.reset_state.reset_cause_procpu`
/// says and as the mask ROM's banner prints it.
///
/// Only the one a direct load can assert is modelled: a cold chip. The
/// ROM's `rtc_get_reset_reason` returns the six-bit field, and **1** is the
/// value L0's silicon banner printed as `rst:0x1 (POWERON_RESET)`
/// (`../bench.md`). The CH340 cable's EN-pin reset is *also* a power-on
/// reset on the classic — the auto-reset circuit pulls EN, not a
/// chip-internal reset line — so unlike the C6 there is no second cause a
/// transcript could start from. P5 owns the register; this is the input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResetCause {
    #[default]
    PowerOn,
}

impl ResetCause {
    /// The value `rtc_get_reset_reason` returns for this cause.
    pub const fn rom_code(self) -> u32 {
        match self {
            ResetCause::PowerOn => 1,
        }
    }

    /// The ROM's own name for it, as the banner prints it.
    pub const fn rom_name(self) -> &'static str {
        match self {
            ResetCause::PowerOn => "POWERON_RESET",
        }
    }
}

/// Where one app segment went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedAppSegment {
    pub vaddr: u32,
    pub paddr: u32,
    pub filesz: u32,
    pub memsz: u32,
    pub execute: bool,
    pub regions: Vec<&'static str>,
}

impl PlacedAppSegment {
    /// `p_vaddr != p_paddr`: placed by vaddr, and recorded rather than
    /// picked silently.
    pub fn relocated(&self) -> bool {
        self.vaddr != self.paddr
    }
}

/// Something wrong with the app image, or with seeding the state around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Rom(RomError),
    /// The entry point is not inside any `PT_LOAD`. (`entry != 0` is not a
    /// usable check in this repository: the guest images under `lp-emu/`
    /// link at zero on purpose.)
    EntryNotLoadable {
        entry: u32,
    },
    /// No loadable segment at all.
    NothingToLoad,
    /// The ROM ELF has no symbol by the name the loader seeds the flash chip
    /// description through — a `--rom` override, or a ROM revision that
    /// renamed it.
    NoFlashChipSymbol {
        symbol: &'static str,
    },
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::Rom(e) => write!(f, "{e}"),
            LoadError::EntryNotLoadable { entry } => write!(
                f,
                "the ELF's entry point {entry:#010x} is not inside any PT_LOAD segment — \
                 this is not a bootable image for this machine"
            ),
            LoadError::NothingToLoad => write!(f, "the ELF has no loadable segment"),
            LoadError::NoFlashChipSymbol { symbol } => write!(
                f,
                "the ROM ELF has no `{symbol}` symbol, so the flash chip's size cannot be \
                 seeded into the ROM's chip description"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<RomError> for LoadError {
    fn from(e: RomError) -> Self {
        LoadError::Rom(e)
    }
}

/// Place the app's segments. The ROM must already be loaded (see the module
/// docs for why the order matters).
pub fn load_app(bus: &mut SocBus, app: &ElfImage) -> Result<Vec<PlacedAppSegment>, LoadError> {
    // The same two skips the ROM loader makes: an empty segment, and a
    // segment whose file bytes are the ELF's own headers (`rom::is_header_map`
    // — the classic ROM has one, and an application linked the same way
    // would too).
    let loadable: Vec<_> = app
        .segments
        .iter()
        .filter(|s| s.memsz > 0 && !rom::is_header_map(&s.data))
        .collect();
    if loadable.is_empty() {
        return Err(LoadError::NothingToLoad);
    }

    let entry_is_loadable = loadable.iter().any(|s| {
        app.entry >= s.vaddr && u64::from(app.entry) < u64::from(s.vaddr) + u64::from(s.memsz)
    });
    if !entry_is_loadable {
        return Err(LoadError::EntryNotLoadable { entry: app.entry });
    }

    let mut placed = Vec::new();
    for seg in loadable {
        if seg.paddr != seg.vaddr {
            // Not an error — a note. The shipped image's `.rtc_fast.persistent`
            // links to RTC fast memory with a load address in the DROM window;
            // a loader that silently used one for the other is how it ends
            // up in the wrong place.
            log::info!(
                "loader: segment at vaddr {:#010x} has paddr {:#010x}; direct load places at \
                 vaddr (the address the running code uses)",
                seg.vaddr,
                seg.paddr
            );
        }
        let regions = place_spanning(bus, seg.vaddr, &seg.data, seg.memsz)?;
        placed.push(PlacedAppSegment {
            vaddr: seg.vaddr,
            paddr: seg.paddr,
            filesz: seg.filesz(),
            memsz: seg.memsz,
            execute: seg.execute,
            regions,
        });
    }
    Ok(placed)
}

/// What [`seed_rom_flash_chip`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashChipSeed {
    /// Where the ROM's chip description is (the symbol's address).
    pub chip: u32,
    /// The `chip_size` that was there before — the ROM's own default.
    pub previous: u32,
    /// The `chip_size` written.
    pub chip_size: u32,
}

/// Tell the mask ROM how big the flash chip is, the way the second-stage
/// bootloader does.
///
/// # The symbol, and why it is not the one the notes name
///
/// `m3/notes.md` §3 item 10 says the classic "does have `g_rom_flashchip`
/// as a symbol". **The vendored ROM ELF does not.** `g_rom_flashchip` is a
/// name ESP-IDF's linker script provides (`esp32.rom.ld`:
/// `PROVIDE ( g_rom_flashchip = 0x3ffae270 )`); the ROM's own symbol table
/// calls the same address **`spi_w25q16`** — a zero-sized `NOTYPE` label at
/// `0x3FFA_E270`, which is also `_data_start_spi_flash`, the start of the
/// 32-byte `.data_spi_flash` section [`crate::rom::seed_data`] seeds. That
/// section *is* the `esp_rom_spiflash_chip_t`: `device_id = 0x001540ef`,
/// `chip_size = 0x0020_0000`, `block_size = 0x1_0000`, `sector_size =
/// 0x1000`, `page_size = 0x100`, `status_mask = 0xffff`, and
/// `dummy_len_plus` after it at `+0x20`. So the loader resolves
/// [`ROM_FLASH_CHIP_SYMBOL`] from the ELF — never a hardcoded address — and
/// the mismatch with the notes is reported, not papered over by adding an
/// alias the ROM does not carry.
///
/// # Why it matters
///
/// The ROM's default is [`ROM_DEFAULT_FLASH_SIZE`], **2 MiB**, and every ROM
/// flash read is bounds-checked against `chip_size` (the classic's
/// `esp_rom_spiflash_read` refuses a read past it, as the C6's
/// `SPI_read_data` does). `lpfs` starts at `0x0031_0000` — past 2 MiB — so
/// with the default every filesystem read returns an error that has
/// nothing to do with the filesystem. On silicon the bootloader calls
/// `esp_rom_spiflash_config_param` with the size from the image header's
/// flash-size field; here the loader writes the same word.
///
/// The ROM's data must already be seeded (the caller places the ROM first);
/// `previous` reports what was there so a test can pin that it was the
/// ROM's own default and not a zero from an unseeded section.
/// Item 11: put the PRO core's read cache in the state the bootloader hands
/// over — **enabled**.
///
/// The one bit, and its citation, is in the module docs. `app_cache_enable`
/// is left as reset leaves it: the bootloader calls `Cache_Read_Enable(0)`,
/// core 1 is not running, and asserting a bit for a core nothing has started
/// would be a claim about a state nobody was in.
///
/// `at` and `pc` are 0: this is the emulator seeding, not a guest store, and
/// the cycle/pc a later stop would quote belong to whatever really turns the
/// bit off.
pub fn seed_cache_enabled(cache: &crate::cache::CacheHandle) {
    let mut c = cache.lock().expect("cache poisoned");
    let ctrl = c.ctrl(0) | crate::cache::CACHE_ENABLE;
    c.write_ctrl(0, ctrl, 0, 0);
}

pub fn seed_rom_flash_chip(
    bus: &mut SocBus,
    rom: &ElfImage,
    chip_size: u32,
) -> Result<FlashChipSeed, LoadError> {
    let chip = rom.symbol(ROM_FLASH_CHIP_SYMBOL).map(|s| s.address).ok_or(
        LoadError::NoFlashChipSymbol {
            symbol: ROM_FLASH_CHIP_SYMBOL,
        },
    )?;
    let at = chip + ROM_FLASH_CHIP_SIZE_OFFSET;
    let previous = read_word(bus, at).unwrap_or(0);
    bus.load_image(at, &chip_size.to_le_bytes()).map_err(|e| {
        LoadError::Rom(RomError::Elf(format!(
            "seeding chip_size at {at:#010x}: {e}"
        )))
    })?;
    Ok(FlashChipSeed {
        chip,
        previous,
        chip_size,
    })
}

// ---------------------------------------------------------------------------
// Staging the image in the chip, and mapping it back — loader item 2
// ---------------------------------------------------------------------------

/// One page a direct load put in the chip and mapped: where it is in the
/// flash window and where its bytes are on the part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedPage {
    pub vaddr: u32,
    pub paddr: u32,
}

/// What [`stage_image_in_flash`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashStaging {
    /// Every page, in ascending virtual address — which is also ascending
    /// flash offset, because that is the packing rule.
    pub pages: Vec<StagedPage>,
    /// How many image bytes were written into the chip.
    pub bytes: u32,
    /// The chip's own length, for the run report.
    pub chip_len: u32,
}

impl FlashStaging {
    /// The flash offset a staged virtual address lives at, or `None` when
    /// the address is in no staged page.
    pub fn paddr_of(&self, vaddr: u32, page_len: u32) -> Option<u32> {
        let base = vaddr & !(page_len - 1);
        self.pages
            .iter()
            .find(|p| p.vaddr == base)
            .map(|p| p.paddr + (vaddr - base))
    }
}

/// Put the application's **flash-resident** segments into the flash chip and
/// program the MMU so the two windows are served through the table.
///
/// # Why this exists at all
///
/// Until P7 the direct load placed the flash windows as plain RAM holding
/// the ELF's bytes and left the MMU empty (the module docs, item 2). That
/// works right up until something asks the flash *chip* a question, and this
/// milestone's firmware does: littlefs mounts `lpfs` at `0x0031_0000`
/// through esp-storage's copy of `esp_rom_spiflash_read`, which reads the
/// same part the app's `.text` lives in. So the chip has to hold the app
/// too, or the two halves of the address space would be describing
/// different boards.
///
/// # The mapping, and why it is this one
///
/// The classic has **two** flash windows — DROM at `0x3F40_0000` and IROM at
/// `0x400D_0000` — where the C6 has one, and the ROM's index arithmetic
/// gives them different index bases (0 and 64) inside one table. There is no
/// single `factory + (vaddr - base)` that serves both without the two
/// windows colliding in the chip. The rule here is instead:
///
/// > every 64 KiB virtual page the image touches, in **ascending virtual
/// > address**, gets the next 64 KiB of the `factory` partition.
///
/// [`FACTORY_OFFSET`] is a whole number of pages and every page maps
/// page-to-page, so `paddr % page == vaddr % page` holds for all of them —
/// which is the constraint `cache_flash_mmu_set` enforces (`bnez a8 →
/// return 1, misaligned`) and the reason esp-hal can link at `0x400D_0020`.
///
/// It is **not** what an `esptool` image would produce: a real flashed image
/// has a header and per-segment headers, and its offsets fall where those
/// leave them. This is the direct-load equivalent of what a flasher did —
/// the bytes are in `factory`, at page-consistent offsets, and the table
/// says where. `tests/rom_up_boot.rs` boots a real merged image through the
/// real bootloader and gets the real offsets; **that** is the cross-check,
/// and this function is one of the things it checks.
///
/// # Both cores' tables
///
/// The ESP-IDF bootloader maps the app for **both** cores
/// (`cache_flash_mmu_set(0, …)` and `cache_flash_mmu_set(1, …)` in
/// `set_cache_and_start_app`), so this writes both — otherwise the
/// cross-check would find a table the two paths disagree about for a reason
/// that is the loader's and not the chip's. The *fill* still serves the PRO
/// core's table only, because this machine has one window behind both
/// (see [`crate::cache::fill`]).
pub fn stage_image_in_flash(
    flash: &FlashHandle,
    cache: &CacheHandle,
    app: &ElfImage,
) -> FlashStaging {
    let page_mode = cache.lock().expect("cache poisoned").page_mode(0);
    let page_len = FlashMmu::page_len(page_mode);
    let mut staging = FlashStaging::default();

    // 1. Which virtual pages the image touches, ascending.
    let mut pages: Vec<u32> = Vec::new();
    for seg in &app.segments {
        if seg.memsz == 0 || flash_window_of(seg.vaddr).is_none() {
            continue;
        }
        let first = seg.vaddr & !(page_len - 1);
        let last = (seg.vaddr + seg.memsz - 1) & !(page_len - 1);
        let mut at = first;
        loop {
            if !pages.contains(&at) {
                pages.push(at);
            }
            if at == last {
                break;
            }
            at += page_len;
        }
    }
    pages.sort_unstable();

    // 2. The next page of `factory` for each, in that order.
    for (i, vaddr) in pages.iter().enumerate() {
        let paddr = FACTORY_OFFSET + (i as u32) * page_len;
        if paddr - FACTORY_OFFSET >= FACTORY_LEN {
            log::warn!(
                "loader: staging {vaddr:#010x} would run past the {FACTORY_LEN:#x}-byte factory \
                 partition; the image does not fit in this chip and the page is not mapped"
            );
            break;
        }
        staging.pages.push(StagedPage {
            vaddr: *vaddr,
            paddr,
        });
    }

    // 3. The bytes, then the `memsz - filesz` tail as zeros — the window
    //    zeroes it, so the chip must too: erased flash is `0xff`, and a
    //    `.bss` that read as ones would not be a `.bss`.
    {
        let mut chip = flash.lock().expect("flash poisoned");
        for seg in &app.segments {
            if seg.memsz == 0 || flash_window_of(seg.vaddr).is_none() {
                continue;
            }
            let tail = seg.memsz.saturating_sub(seg.data.len() as u32);
            let zeros = vec![0u8; tail as usize];
            for (at, bytes) in [
                (seg.vaddr, seg.data.as_slice()),
                (seg.vaddr + seg.data.len() as u32, zeros.as_slice()),
            ] {
                let mut written = 0u32;
                while written < bytes.len() as u32 {
                    let vaddr = at + written;
                    let Some(paddr) = staging.paddr_of(vaddr, page_len) else {
                        break;
                    };
                    let in_page = page_len - (vaddr & (page_len - 1));
                    let n = in_page.min(bytes.len() as u32 - written);
                    let slice = &bytes[written as usize..(written + n) as usize];
                    if chip.stage(paddr, slice) && at == seg.vaddr {
                        staging.bytes += n;
                    }
                    written += n;
                }
            }
        }
        staging.chip_len = chip.len();
    }

    // 4. The table, both cores.
    let mut c = cache.lock().expect("cache poisoned");
    for page in &staging.pages {
        let Some(index) = FlashMmu::entry_index(page.vaddr, page_mode) else {
            log::warn!("loader: {:#010x} is in no flash window", page.vaddr);
            continue;
        };
        let entry = page.paddr >> FlashMmu::shift(page_mode);
        for core in 0..crate::cache::CORES {
            c.mmu.set_entry(core, index as usize, entry);
        }
    }
    staging
}

/// Which flash window an address is in, as `(name, base)`, or `None`.
pub fn flash_window_of(vaddr: u32) -> Option<(&'static str, u32)> {
    if (memmap::DROM_BASE..memmap::DROM_BASE + memmap::DROM_LEN).contains(&vaddr) {
        return Some(("drom", memmap::DROM_BASE));
    }
    if (memmap::IROM_BASE..memmap::IROM_BASE + memmap::IROM_LEN).contains(&vaddr) {
        return Some(("irom", memmap::IROM_BASE));
    }
    None
}

/// Read one little-endian word out of a RAM region from the host side, or
/// `None` if the address is not in one. The small counterpart of
/// `SocBus::load_image`, for the loader and its tests.
pub fn read_word(bus: &SocBus, address: u32) -> Option<u32> {
    let region = bus
        .regions()
        .iter()
        .find(|r| r.contains(address) && r.contains(address + 3))?;
    let at = (address - region.base) as usize;
    let bytes = &bus.region_bytes(region)[at..at + 4];
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// The ROM stacks and `dram_seg` are plain RAM here — nothing to seed, but
/// the fact is asserted so it stays true: the regions exist and are
/// writable. `memmap` declares them; this is the loader saying it relies on
/// that.
pub fn plain_ram_regions() -> [(&'static str, u32, u32); 3] {
    [
        ("dram-seg", memmap::DRAM_SEG_BASE, memmap::DRAM_SEG_LEN),
        (
            "rom pro stack",
            memmap::ROM_PRO_STACK_BASE,
            memmap::ROM_STACK_LEN,
        ),
        (
            "rom app stack",
            memmap::ROM_APP_STACK_BASE,
            memmap::ROM_STACK_LEN,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bootloader_sp_is_the_rom_stack_minus_the_frame_chain() {
        let used: u32 = BOOTLOADER_FRAME_CHAIN.iter().map(|(_, _, n)| n).sum();
        assert_eq!(used, 672, "112 + 192 + 304 + 64");
        assert_eq!(
            memmap::ROM_PRO_STACK_TOP - used,
            BOOTLOADER_SP_AT_APP_ENTRY,
            "the pinned constant is __stack minus the four entry frames"
        );
        assert_eq!(BootFrame::idf_bootloader().sp, BOOTLOADER_SP_AT_APP_ENTRY);
        // Every frame is a whole number of 16-byte units, as `entry` requires
        // (imm12 << 3, and the ABI keeps frames 16-aligned).
        for (who, _, n) in BOOTLOADER_FRAME_CHAIN {
            assert_eq!(n % 16, 0, "{who}");
        }
    }

    #[test]
    fn power_on_is_the_code_the_banner_printed() {
        assert_eq!(ResetCause::PowerOn.rom_code(), 1);
        assert_eq!(ResetCause::PowerOn.rom_name(), "POWERON_RESET");
    }
}
