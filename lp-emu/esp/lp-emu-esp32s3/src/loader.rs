//! The direct load: the application's `PT_LOAD`s, placed by vaddr.
//!
//! # What a direct load is, and what it is not
//!
//! It is the shortest path from a `fw-esp32s3` ELF to a running hart: place
//! every loadable segment where the linker said it lives, seed core 0's
//! entry, `PS` and a boot frame ([`crate::machine::BootFrame`]), and run. It
//! is how a bring-up iterates in seconds instead of in a whole boot chain,
//! and it is what M6 P03 through P05 use.
//!
//! It is **not** a boot. What a real ESP32-S3 power-on does that this does
//! not — **the direct-load ↔ ROM-up cross-check's seed**, since P06 boots
//! the same image from the reset vector through the real ROM and the real
//! IDF bootloader (`tests/rom_up_boot.rs`), and every line below is a place
//! the two paths can disagree:
//!
//! 1. **No partition table** is read or validated; no `esp_app_desc`,
//!    image-hash, secure-boot or flash-encryption path. A corrupt image
//!    boots here and does not there.
//! 2. **The flash MMU page table is programmed by the loader, not by the
//!    bootloader** ([`stage_image_in_flash`], P06): every 64 KiB virtual
//!    page the image touches gets the next 64 KiB of `factory`, which is
//!    the loader's arithmetic and not an `esptool` image's segment layout.
//!    The cross-check compares the two tables and the bytes behind them.
//! 3. **The ROM console is never initialised.** `uartAttach`,
//!    `ets_install_uart_printf` and the printf-channel globals are untouched,
//!    and neither `g_usb_print` nor `g_uart_print` is set.
//! 4. **No ROM banner** (`ESP-ROM:esp32s3-20210327`, `rst:0x1
//!    (POWERON),boot:0x8 (SPI_FAST_FLASH_BOOT)`, the `load:` lines) **and no
//!    bootloader log** — a large part of what a boot transcript compares,
//!    and none of it exists on this path.
//! 5. **No early RNG entropy** (the bootloader's *"Enabling RNG early entropy
//!    source"* step); the machine's seeded PRNG stands in.
//! 6. **eFuse is asserted, not read.** The MAC and the wafer version come
//!    from whatever the EFUSE block is seeded with (**P04**, and a zero
//!    identity until P09 reads a board).
//! 7. **The reset cause is asserted as POWERON** ([`ResetCause`], **P04**).
//!    Nothing consulted `RTC_CNTL.reset_state` to decide it.
//! 8. **The RTC watchdog is armed by the machine**, where silicon's ROM
//!    leaves it armed (**P04**); the shipped image feeds it on every boot
//!    either way (`m6/notes.md` §5.2).
//! 9. **`CPENABLE` is a parameter** ([`crate::machine::Esp32S3Builder::
//!    cpenable_reset`], **P09** pins it), where the boot chain leaves
//!    whatever it leaves.
//! 10. **The strapping pins are asserted** — `GPIO.strap` reads `--strap`'s
//!     word, default [`crate::periph::accept::GPIO_STRAP_SPI_FAST_FLASH_BOOT`]
//!     — and the direct load never reads them; the ROM-up boot is their
//!     first reader (P06).
//! 11. **Both caches are left enabled** ([`seed_cache_enabled`]), which is
//!     what the IDF bootloader hands over: `load_image` calls
//!     `cache_hal_enable(CACHE_TYPE_ALL)` (`403cd976: movi.n a10, 2; call8
//!     403cebd0` → `Cache_Enable_DCache` / `Cache_Enable_ICache`) two
//!     instructions before the `callx8` into the app. Without the seed D4's
//!     stop fires on the app's first IROM fetch — the emulator being right
//!     about its own state and wrong about silicon's.
//! 12. **The flash chip's `chip_size`** is written by the loader
//!     ([`seed_rom_flash_chip`]) in place of the bootloader's
//!     `esp_rom_spiflash_config_param`. Default 8 MiB.
//! 13. **`.data` is placed rather than copied** — the app's own self-copy is
//!     a no-op (`_sidata == _data_start == 0x3fc8_b320`, `nm` on the shipped
//!     image), so a mis-placed `.data` cannot be repaired by the guest.
//! 14. **The stack.** The app is entered on the frame the IDF bootloader's
//!     `callx8` leaves — [`BOOTLOADER_SP_AT_APP_ENTRY`], derived below and
//!     measured by the cross-check — with `PS.OWB` and the save area
//!     [`crate::machine::BootFrame::idf_bootloader`] carries.
//!
//! Every one of those is a *difference a run can show*, which is why the list
//! is here rather than in a commit message: a run that disagrees with
//! silicon disagrees for a reason on this list, or for a reason that is a
//! finding.
//!
//! # The stack pointer at the application's entry, derived
//!
//! P03 seeded `a1` = the mask ROM's `__stack` (`0x3FCE_B710`) and said the
//! bootloader's own SP at the `callx8` into the app was P06's to derive. It
//! is derived here from the disassembly of both halves of the chain, and
//! the chain is [`BOOTLOADER_FRAME_CHAIN`]:
//!
//! ```text
//! mask ROM  _start            0x40034c02  l32r a1, __stack      a1 = 0x3FCE_B710
//!           0x40034c45        call4 main
//! mask ROM  main              0x40043a2c  entry a1, 32          a1 = 0x3FCE_B6F0
//!           0x40043cad        callx8 [user_code_start]   (0x3fcedf14 → the bootloader's entry)
//! IDF bl    call_start_cpu0   0x403c9908  entry a1, 192         a1 = 0x3FCE_B630
//!           0x403c9971        call8 0x403cdd54
//! IDF bl    …load_boot_image  0x403cdd54  entry a1, 304         a1 = 0x3FCE_B500
//!           0x403cdd79        call8 0x403cd8ac
//! IDF bl    load_image        0x403cd8ac  entry a1, 64          a1 = 0x3FCE_B4C0
//!           0x403cd980        callx8 a9                  (the app's e_entry, from the stack: `l32i.n a9, a1, 24`)
//! ```
//!
//! So the application's `Reset` is entered with **`a1 = 0x3FCE_B4C0`** and
//! `PS.CALLINC = 2`. The bootloader never sets a stack pointer of its own;
//! it runs on the ROM's PRO stack, 592 bytes down. The ROM half is read
//! from the vendored `esp32s3_rev0_rom.elf` with symbols; the bootloader
//! half from the bootloader `espflash save-image --chip esp32s3 --merge`
//! embeds (ESP-IDF `v5.1-beta1-378-gea5e0ff298-dirt`, entry `0x403c_9908`,
//! IRAM segments at `0x403c_9704` and `0x403c_c700`), disassembled raw with
//! `xtensa-esp32s3-elf-objdump -D -b binary -m xtensa --adjust-vma`. The
//! IDF names are from the source (`bootloader_start.c: call_start_cpu0`,
//! `bootloader_utility.c: bootloader_utility_load_boot_image` → `load_image`
//! → the inlined `set_cache_and_start_app`); the addresses are what the
//! binary says, and the `callx8 a9` at `0x403cd980` sits two instructions
//! after `cache_hal_enable(CACHE_TYPE_ALL)` and one after
//! `bootloader_atexit`'s `uart_tx_flush` (`403cdf0c` → ROM `0x4000_0690`),
//! which is what a `noreturn` call into the app looks like.
//!
//! The four words of the base save area at `[a1-16, a1)` and `PS.OWB` are
//! **not** derived here; `tests/rom_up_boot.rs`'s cross-check, which arrives
//! at the same `callx8` with the real values in the register file, is the
//! measurement, and [`BOOTLOADER_SAVE_AREA`] / [`BOOTLOADER_OWB`] carry what
//! it read.
//!
//! # Placed by `p_vaddr`, and the S3 makes that matter
//!
//! The shipped image's `.rtc_fast.persistent` links to RTC fast memory at
//! `0x600F_E000` with a load address in the DROM window (`m6/notes.md` §2.6
//! lists `0x600f_e000 RW` as its own `PT_LOAD`). A loader that placed by
//! `p_paddr` would put `lp_recovery`'s ledger in flash and the firmware would
//! read zeros from the address it actually uses. So placement is by vaddr —
//! the address the running code names — and a segment whose two differ is
//! **logged**, not silently resolved.

use lp_emu_esp_common::{ElfImage, SocBus};

use crate::cache::{CacheHandle, FlashMmu, MMU_INVALID, MMU_PAGE_MASK, PAGE_LEN, PAGE_SHIFT};
use crate::flash::{FACTORY_LEN, FACTORY_OFFSET, FlashHandle};
use crate::machine::BootFrame;
use crate::memmap;
use crate::rom::{self, RomError, place_spanning};

/// The stack pointer the IDF bootloader hands the application: the mask
/// ROM's `__stack` minus the four frames between `_start` and the `callx8`
/// into the app. See the module docs for the derivation and
/// [`BOOTLOADER_FRAME_CHAIN`] for the frames.
pub const BOOTLOADER_SP_AT_APP_ENTRY: u32 = 0x3FCE_B4C0;

/// One frame between the ROM's `__stack` and the app's entry:
/// `(who, the pc of its `entry`, the bytes that `entry` reserves)`.
///
/// `the_bootloader_sp_is_the_rom_stack_minus_the_frame_chain` re-derives
/// [`BOOTLOADER_SP_AT_APP_ENTRY`] from this table and
/// [`memmap::ROM_PRO_STACK_TOP`], so the constant and its derivation cannot
/// drift apart.
pub const BOOTLOADER_FRAME_CHAIN: &[(&str, u32, u32)] = &[
    ("mask ROM main", 0x4004_3A2C, 32),
    ("IDF bootloader call_start_cpu0", 0x403C_9908, 192),
    (
        "IDF bootloader bootloader_utility_load_boot_image",
        0x403C_DD54,
        304,
    ),
    ("IDF bootloader load_image", 0x403C_D8AC, 64),
];

/// The four words at `[a1-16, a1)` when the ESP-IDF second-stage bootloader
/// reaches the application's entry — `a0, a1, a2, a3` of the frame behind
/// the app's, in the order `_WindowOverflow4` spills them.
///
/// **A coherent frame until measured.** `tests/rom_up_boot.rs::
/// rom_up_and_direct_load_agree_on_what_the_app_sees` stops both paths at
/// the app's first instruction, reads the ROM-up side's stack, and asserts
/// the direct load's seed against it; this constant is what that assertion
/// holds, and the run that first read the real words is the one that sets
/// it. `a0 = 0` is the frame chain's end and the saved `a1` chains to the
/// stack top, so a spill through the frame lands in ROM-owned memory.
/// Nothing in the application reads these before its first window overflow
/// overwrites them.
pub const BOOTLOADER_SAVE_AREA: [u32; 4] = [0x0000_0000, BOOTLOADER_SP_AT_APP_ENTRY, 0, 0];

/// `PS.OWB` at the application's entry. **Zero until measured** by the same
/// cross-check: the architectural value for a frame no window exception has
/// touched. The classic's 7 is the classic's measurement and is not carried
/// over.
pub const BOOTLOADER_OWB: u8 = 0;

impl BootFrame {
    /// The frame the IDF bootloader leaves at the application's entry:
    /// `a1 = ` [`BOOTLOADER_SP_AT_APP_ENTRY`], the measured
    /// [`BOOTLOADER_SAVE_AREA`] and [`BOOTLOADER_OWB`]. The direct load's
    /// default since P06.
    pub const fn idf_bootloader() -> Self {
        Self {
            sp: BOOTLOADER_SP_AT_APP_ENTRY,
            save_area: BOOTLOADER_SAVE_AREA,
            owb: BOOTLOADER_OWB,
        }
    }
}

/// What `RTC_CNTL.reset_state` says the chip is starting from — loader item
/// 6, and an **input to the run** rather than a property of the part.
///
/// The mask ROM's `rtc_get_reset_reason` (`0x4004_456C`) reads `+0x38` and
/// masks six bits per core (`extui a2, a2, 0, 6` for the PRO core, `6, 6`
/// for the APP core); the PAC's reset for the register is `0x3000`, both
/// cause fields zero, because an SVD cannot know why a chip is starting.
/// A direct load asserts a power-on for both cores. There is one variant
/// because a direct load is the only way this machine starts in M6 P04; a
/// watchdog reset is *reported* ([`crate::machine::Outcome::Reset`]) and
/// not yet performed, so no run begins from one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResetCause {
    #[default]
    PowerOn,
}

impl ResetCause {
    /// The value `rtc_get_reset_reason` returns for this cause: `1` is
    /// `POWERON_RESET` on every Espressif part, the code the ROM's banner
    /// prints as `rst:0x1 (POWERON_RESET)`.
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

/// What the eFuse block reports — loader item 7. The eFuse view is built
/// from it ([`crate::periph::efuse`]), and `--efuse-mac` / `--efuse-rev`
/// set it.
///
/// ⚠️ **There is no measured S3 fuse dump yet, and this type does not
/// pretend otherwise.** The classic's `EfuseIdentity` defaults to the desk
/// board's seven `espefuse` words; the C6's to a MAC read over the download
/// console. Neither has been done for an S3 board, so the default here is
/// **the PAC's own resets — a zero MAC and wafer version 0.0** — which is
/// not a plausible value but the absence of one, and reads as such in the
/// firmware's own identity line. Seeding something that *looks* right is
/// exactly the classic's `CLK8M_FREQ` lesson: a legal, silent zero that made
/// the ROM conclude 26 MHz for a 40 MHz board. **TODO(P09): replace the
/// default with the desk S3's read fuse dump** and promote the words to
/// `measured`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EfuseIdentity {
    pub mac: [u8; 6],
    /// `WAFER_VERSION_MAJOR`, two bits.
    pub wafer_major: u8,
    /// `WAFER_VERSION_MINOR`, four bits — split across two eFuse words on
    /// this chip ([`crate::periph::efuse`]).
    pub wafer_minor: u8,
}

impl EfuseIdentity {
    /// Parse `aa:bb:cc:dd:ee:ff`.
    pub fn parse_mac(text: &str) -> Result<[u8; 6], String> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 6 {
            return Err(format!("`{text}` is not six colon-separated octets"));
        }
        let mut mac = [0u8; 6];
        for (i, p) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(p, 16)
                .map_err(|_| format!("`{p}` in `{text}` is not a hex octet"))?;
        }
        Ok(mac)
    }

    /// Parse `major.minor` (`0.2`, `1.0`).
    pub fn parse_rev(text: &str) -> Result<(u8, u8), String> {
        let (major, minor) = text
            .split_once('.')
            .ok_or_else(|| format!("`{text}` is not `major.minor`"))?;
        let major: u8 = major
            .parse()
            .map_err(|_| format!("`{major}` in `{text}` is not a number"))?;
        let minor: u8 = minor
            .parse()
            .map_err(|_| format!("`{minor}` in `{text}` is not a number"))?;
        if major > 3 {
            return Err(format!("`{text}`: WAFER_VERSION_MAJOR is two bits"));
        }
        if minor > 15 {
            return Err(format!("`{text}`: WAFER_VERSION_MINOR is four bits"));
        }
        Ok((major, minor))
    }
}

/// One application segment, placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedAppSegment {
    pub vaddr: u32,
    pub paddr: u32,
    pub filesz: u32,
    pub memsz: u32,
    pub execute: bool,
    /// The region names the bytes landed in, in address order.
    ///
    /// ⚠️ An executable segment at `0x4037_8000` reports **`sram1-dbus`**,
    /// not an I-bus name, and that is correct: the I-bus view is a RAM alias
    /// and `SocBus` translates to the canonical address before anything looks
    /// a region up (DD81). The region a byte lives in is the D-bus one
    /// whichever door it arrived through.
    pub regions: Vec<&'static str>,
}

impl PlacedAppSegment {
    /// `p_vaddr != p_paddr`: placed by vaddr, and recorded rather than picked
    /// silently. See the module docs for the S3 segment where this bites.
    pub fn relocated(&self) -> bool {
        self.vaddr != self.paddr
    }
}

/// Something wrong with the app image, or with seeding the state around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Rom(RomError),
    /// The entry point is not inside any `PT_LOAD`. (`entry != 0` is not a
    /// usable check in this repository: the guest images under `lp-emu/` link
    /// at zero on purpose.)
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
    /// The pointer the ROM keeps at that symbol does not point into mapped
    /// memory — the ROM's `.data.interface` was not seeded, or a `--rom`
    /// override carries a different layout.
    FlashChipPointer {
        symbol: &'static str,
        pointer: u32,
    },
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::Rom(e) => write!(f, "{e}"),
            LoadError::EntryNotLoadable { entry } => write!(
                f,
                "the ELF's entry point {entry:#010x} is not inside any PT_LOAD segment — this \
                 is not a bootable image for this machine"
            ),
            LoadError::NothingToLoad => write!(f, "the ELF has no loadable segment"),
            LoadError::NoFlashChipSymbol { symbol } => write!(
                f,
                "the ROM ELF has no `{symbol}` symbol, so the flash chip's size cannot be \
                 seeded into the ROM's chip description"
            ),
            LoadError::FlashChipPointer { symbol, pointer } => write!(
                f,
                "`{symbol}` holds {pointer:#010x}, which is not a mapped address; the ROM's \
                 chip description cannot be found through it"
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

/// Place the app's segments. The ROM must already be loaded — a direct load
/// overwrites what the app owns exactly as a real bootloader would, and the
/// ROM's own `.data_*` sections go down first.
pub fn load_app(bus: &mut SocBus, app: &ElfImage) -> Result<Vec<PlacedAppSegment>, LoadError> {
    // The same two skips the ROM loader makes: an empty segment, and a
    // segment whose file bytes are the ELF's own headers
    // (`rom::is_header_map` — the S3 mask ROM has one, and an application
    // linked the same way would too).
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

/// Read one little-endian word out of a RAM region from the host side, or
/// `None` if the address is not in one. The small counterpart of
/// [`SocBus::load_image`], for the loader and its tests.
pub fn read_word(bus: &SocBus, address: u32) -> Option<u32> {
    let region = bus
        .regions()
        .iter()
        .find(|r| r.contains(address) && r.contains(address + 3))?;
    let at = (address - region.base) as usize;
    let bytes = &bus.region_bytes(region)[at..at + 4];
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

// ---------------------------------------------------------------------------
// The flash chip, as the ROM sees it — loader item 12
// ---------------------------------------------------------------------------

/// The ROM's own name for the **pointer** to its SPI flash chip description.
/// See [`seed_rom_flash_chip`].
pub const ROM_FLASH_LEGACY_DATA_SYMBOL: &str = "rom_spiflash_legacy_data";

/// The byte offset of `chip_size` inside the chip description the pointer
/// names: the second word of `esp_rom_spiflash_chip_t` (`device_id,
/// chip_size, block_size, sector_size, page_size, status_mask`), which is
/// the layout `esp_rom_spiflash_config_param` (`0x4004_aeb4`) writes —
/// `4004aebc: s32i.n a2, a8, 0` … `4004aec6: s32i.n a7, a8, 20`, six words
/// from the pointer's target.
pub const ROM_FLASH_CHIP_SIZE_OFFSET: u32 = 4;

/// The ROM's default `chip_size`: `0x0020_0000`, **2 MiB**, read out of
/// `.data_spi_flash` in the vendored ELF (`objdump -s -j .data_spi_flash`:
/// `ef401500 00002000 …` at `0x3fcef6a4`). `lpfs` starts at `0x0061_0000`,
/// past it, which is why the seed exists.
pub const ROM_DEFAULT_FLASH_SIZE: u32 = 0x0020_0000;

/// What [`seed_rom_flash_chip`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashChipSeed {
    /// Where the ROM's chip description is — the target of the pointer at
    /// [`ROM_FLASH_LEGACY_DATA_SYMBOL`].
    pub chip: u32,
    /// The `chip_size` that was there before — the ROM's own default.
    pub previous: u32,
    /// The `chip_size` written.
    pub chip_size: u32,
}

/// Tell the mask ROM how big the flash chip is, the way the second-stage
/// bootloader does.
///
/// # The symbol, and why it is a pointer
///
/// On this ROM the chip description is reached through
/// **`rom_spiflash_legacy_data`** (`0x3fce_ffe4`, the first word of
/// `.data.interface.spiflash_legacy`), which holds a *pointer* —
/// `0x3fce_f6a4`, into `.data_spi_flash` — to the `esp_rom_spiflash_chip_t`
/// every ROM flash routine loads first (`_esp_rom_spiflash_read`
/// `4004abf3: l32r a5, (3fceffe4)` / `4004abf6: l32i a10, a5, 0`;
/// `esp_rom_spiflash_config_param` `4004aeb7..aeba`). Both the pointer and
/// the structure it names are non-alloc `W` sections
/// [`crate::rom::seed_data`] places, so the loader resolves the symbol from
/// the ELF, **reads the pointer out of the seeded bus** rather than
/// assuming its target, and writes `chip_size` at `+4`.
///
/// # Why it matters
///
/// The ROM's default is [`ROM_DEFAULT_FLASH_SIZE`], **2 MiB**, and every ROM
/// flash read is bounds-checked against `chip_size` — `SPI_read_data`
/// `400498d3: l32i.n a7, a2, 4` / `400498d9: bgeu a7, a6, ok` else
/// **return 1**, the same on `_esp_rom_spiflash_write` (`4004ab3d..3f`) and
/// `_esp_rom_spiflash_erase_sector` (`4004aaa9..b1`). `lpfs` starts at
/// `0x0061_0000` — past 2 MiB — so with the default every filesystem read
/// returns an error that has nothing to do with the filesystem. On silicon
/// the bootloader calls `esp_rom_spiflash_config_param` with the size from
/// the image header's flash-size nibble; here the loader writes the same
/// word.
///
/// The ROM's data must already be seeded (the caller places the ROM first);
/// `previous` reports what was there so a test can pin that it was the
/// ROM's own default and not a zero from an unseeded section.
pub fn seed_rom_flash_chip(
    bus: &mut SocBus,
    rom: &ElfImage,
    chip_size: u32,
) -> Result<FlashChipSeed, LoadError> {
    let symbol = ROM_FLASH_LEGACY_DATA_SYMBOL;
    let slot = rom
        .symbol(symbol)
        .map(|s| s.address)
        .ok_or(LoadError::NoFlashChipSymbol { symbol })?;
    let pointer = read_word(bus, slot).ok_or(LoadError::FlashChipPointer { symbol, pointer: 0 })?;
    let at = pointer.wrapping_add(ROM_FLASH_CHIP_SIZE_OFFSET);
    let previous = read_word(bus, at).ok_or(LoadError::FlashChipPointer { symbol, pointer })?;
    bus.load_image(at, &chip_size.to_le_bytes()).map_err(|e| {
        LoadError::Rom(RomError::Elf(format!(
            "seeding chip_size at {at:#010x}: {e}"
        )))
    })?;
    Ok(FlashChipSeed {
        chip: pointer,
        previous,
        chip_size,
    })
}

// ---------------------------------------------------------------------------
// Staging the image in the chip, and mapping it back — loader item 2
// ---------------------------------------------------------------------------

/// One page a direct load put in the chip and mapped: where it is in the
/// flash windows (the entry index serves both) and where its bytes are on
/// the part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedPage {
    /// The virtual page as the ELF names it — in whichever window.
    pub vaddr: u32,
    /// Its MMU entry index, `(vaddr >> 16) & 0x1ff`.
    pub index: u32,
    pub paddr: u32,
}

/// What [`stage_image_in_flash`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashStaging {
    /// Every page, in ascending virtual address — which is also ascending
    /// flash offset, because that is the packing rule.
    pub pages: Vec<StagedPage>,
    /// Virtual pages of the image that **share** an entry already staged
    /// from the other window — `(vaddr, index)`. The linker's
    /// `.rotext_dummy` (module docs of [`stage_image_in_flash`]) is the one
    /// producer; nothing is copied for them, because the entry's bytes are
    /// the other window's page.
    pub shadows: Vec<(u32, u32)>,
    /// How many image bytes were written into the chip.
    pub bytes: u32,
    /// The chip's own length, for the run report.
    pub chip_len: u32,
}

impl FlashStaging {
    /// The flash offset a staged virtual address lives at, or `None` when
    /// the address is in no staged page (a shadow page included: its bytes
    /// are the other window's).
    pub fn paddr_of(&self, vaddr: u32) -> Option<u32> {
        let base = vaddr & !(PAGE_LEN - 1);
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
/// Until P06 the direct load placed the flash windows as plain RAM holding
/// the ELF's bytes and left no chip behind them. That works right up until
/// something asks the flash *chip* a question, and this firmware does:
/// littlefs mounts `lpfs` at `0x0061_0000` through esp-storage's
/// `esp_rom_spiflash_read`, which reads the same part the app's `.text`
/// lives in. So the chip has to hold the app too, or the two halves of the
/// address space would be describing different boards.
///
/// # The mapping, and why it is this one
///
/// > every 64 KiB virtual page the image touches, in **ascending virtual
/// > address**, gets the next 64 KiB of the `factory` partition.
///
/// [`FACTORY_OFFSET`] is a whole number of pages and every page maps
/// page-to-page, so `paddr % 64K == vaddr % 64K` holds — the constraint
/// `Cache_Ibus_MMU_Set` enforces (`4004f728: and a8, a8, (vaddr|paddr)` →
/// return 2) and the reason esp-hal can link at `0x4205_0020`.
///
/// ⚠️ **One table serves both windows on this chip**, so a DROM page and an
/// IROM page with the same `(vaddr >> 16) & 0x1ff` are **one entry** — and
/// the shipped image has five such pairs on purpose. esp-hal's S3 linker
/// script opens the IROM segment with `.rotext_dummy`, a `NOBITS` section
/// exactly the size of the DROM pages (`0x42000020`, `0x50000` on the shipped
/// image: `readelf -S`), so that `.text` starts at `0x4205_0020` — entry 5,
/// clear of `.rodata`'s entries 0..4. On silicon the first five IROM pages
/// *are* the rodata pages, read through the instruction bus, which is what
/// the dummy exists to keep `.text` off. So a virtual page whose index is
/// already staged from the other window is a **shadow**: it is recorded
/// ([`FlashStaging::shadows`]), no flash is spent on it, nothing is copied
/// for it, and the fill serves both windows from the one entry — exactly
/// the silicon reading. (The direct loader also zero-fills the `NOBITS`
/// span into the IROM window first; the fill then overwrites it with the
/// rodata bytes, as the ROM-up path's cache would show.)
///
/// It is **not** what an `esptool` image would produce: a real flashed image
/// has a header and per-segment headers, and its offsets fall where those
/// leave them. `tests/rom_up_boot.rs` boots a real merged image through the
/// real bootloader and gets the real offsets; **that** is the cross-check,
/// and this function is one of the things it checks.
pub fn stage_image_in_flash(
    flash: &FlashHandle,
    cache: &CacheHandle,
    app: &ElfImage,
) -> FlashStaging {
    let mut staging = FlashStaging::default();

    // 1. Which virtual pages the image touches, ascending — so the DROM
    //    pages (0x3C…) come before the IROM pages (0x42…) and a shadow is
    //    always the IROM side, as the linker intends.
    let mut pages: Vec<u32> = Vec::new();
    for seg in &app.segments {
        if seg.memsz == 0 || flash_window_of(seg.vaddr).is_none() {
            continue;
        }
        let first = seg.vaddr & !(PAGE_LEN - 1);
        let last = (seg.vaddr + seg.memsz - 1) & !(PAGE_LEN - 1);
        let mut at = first;
        loop {
            if !pages.contains(&at) {
                pages.push(at);
            }
            if at == last {
                break;
            }
            at += PAGE_LEN;
        }
    }
    pages.sort_unstable();

    // 2. The next page of `factory` for each, in that order; a page on an
    //    index already taken shares it.
    let mut used: Vec<u32> = Vec::new();
    for vaddr in &pages {
        let Some(index) = FlashMmu::entry_index(*vaddr) else {
            log::warn!("loader: {vaddr:#010x} is in no flash window");
            continue;
        };
        if used.contains(&index) {
            log::debug!(
                "loader: virtual page {vaddr:#010x} shares MMU entry {index} with the other \
                 window's page (the linker's .rotext_dummy); no flash is staged for it"
            );
            staging.shadows.push((*vaddr, index));
            continue;
        }
        let paddr = FACTORY_OFFSET + (staging.pages.len() as u32) * PAGE_LEN;
        if paddr - FACTORY_OFFSET >= FACTORY_LEN {
            log::warn!(
                "loader: staging {vaddr:#010x} would run past the {FACTORY_LEN:#x}-byte factory \
                 partition; the image does not fit in this chip and the page is not mapped"
            );
            break;
        }
        used.push(index);
        staging.pages.push(StagedPage {
            vaddr: *vaddr,
            index,
            paddr,
        });
    }

    // 3. The bytes, then the `memsz - filesz` tail as zeros — the window
    //    zeroes it, so the chip must too: erased flash is `0xff`, and a
    //    `.bss` that read as ones would not be a `.bss`. A shadow page is
    //    skipped, not written: its bytes are the other window's.
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
                    let in_page = PAGE_LEN - (vaddr & (PAGE_LEN - 1));
                    let n = in_page.min(bytes.len() as u32 - written);
                    if let Some(paddr) = staging.paddr_of(vaddr) {
                        let slice = &bytes[written as usize..(written + n) as usize];
                        if chip.stage(paddr, slice) && at == seg.vaddr {
                            staging.bytes += n;
                        }
                    }
                    written += n;
                }
            }
        }
        staging.chip_len = chip.len();
    }

    // 4. The table: every entry invalid first — what `Cache_MMU_Init` leaves
    //    and what the bootloader runs before mapping the app — then the
    //    staged pages.
    let mut c = cache.lock().expect("cache poisoned");
    for index in 0..crate::cache::MMU_ENTRIES {
        c.mmu.set_entry(index, MMU_INVALID);
    }
    for page in &staging.pages {
        c.mmu.set_entry(
            page.index as usize,
            (page.paddr >> PAGE_SHIFT) & MMU_PAGE_MASK,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bootloader_sp_is_the_rom_stack_minus_the_frame_chain() {
        let used: u32 = BOOTLOADER_FRAME_CHAIN.iter().map(|(_, _, n)| n).sum();
        assert_eq!(used, 592, "32 + 192 + 304 + 64");
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
        // And the ROM half of the chain is the vendored ROM's own `main`.
        let rom = crate::rom::vendored().expect("the vendored ROM parses");
        assert_eq!(
            rom.symbol("main").map(|s| s.address),
            Some(BOOTLOADER_FRAME_CHAIN[0].1)
        );
        assert_eq!(
            rom.symbol("__stack").map(|s| s.address),
            Some(memmap::ROM_PRO_STACK_TOP)
        );
    }

    #[test]
    fn the_windows_are_the_two_the_table_serves() {
        assert_eq!(
            flash_window_of(0x3C00_0020),
            Some(("drom", memmap::DROM_BASE))
        );
        assert_eq!(
            flash_window_of(0x4205_0020),
            Some(("irom", memmap::IROM_BASE))
        );
        assert_eq!(flash_window_of(0x4037_8000), None);
        assert_eq!(flash_window_of(0x3FC8_B320), None);
    }
}
