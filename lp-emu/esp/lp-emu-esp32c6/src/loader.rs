//! Direct load — start at `_start` with the machine in the state the ROM and
//! the ESP-IDF second-stage bootloader would have left it in.
//!
//! This is the `--skip-rom` equivalent, and the reason it is not simply "put
//! the ELF in memory and jump" is the vision's line: **the bootloader matters
//! for memory.** Its `iram_loader_seg` reclaim is the app's second heap
//! region, `.data` is not copied by the app because the bootloader is
//! expected to have done it, and the core arrives at `_start` with
//! `mstatus.MIE` already 1 — nothing in the esp-hal stack ever sets it, so a
//! hart left at the architectural reset value would idle in `wfi` forever
//! (discovery §1h).
//!
//! # What this reproduces
//!
//! - Every `PT_LOAD` at its **`vaddr`**, with the `memsz - filesz` tail
//!   zeroed. The shipped C6 image has `vaddr == paddr` on all seven of its
//!   loadable segments, so there is nothing to choose between; the loader
//!   records any segment where they differ instead of quietly picking one.
//! - `.data` **placed, not copied**: `hal-defaults.x:53-56` hardcodes
//!   `__sdata = __edata = __sidata = 0` with the comment "don't init data —
//!   expect the bootloader to do it", so `_start`'s copy loop is a no-op and
//!   the bytes have to already be there. (`.bss` *is* zeroed by the app, and
//!   `.rtc_fast.bss` by `__pre_init`; both are zero here anyway.)
//! - `dram2_seg` (`0x4086_E610..0x4087_E610`) as **plain zeroed RAM**. There
//!   is nothing to seed: esp-alloc's second region is an ordinary
//!   `static MaybeUninit` placed there by `#[esp_hal::ram(reclaimed)]`
//!   (`init.rs:45`). The requirement is only that the bytes are RAM rather
//!   than the bootloader's own image, which is exactly what "the bootloader
//!   finished and went away" means.
//! - The mask ROM, loaded **first**, so its `.bss` (which reaches from
//!   `0x4086_ad08` across what the app calls RAM) is overwritten by the app's
//!   own segments — the order the real bootloader runs in.
//! - Hart reset: `pc = e_entry`, all GPRs zero, `mstatus = 0x1888`,
//!   `mtvec = 0`, `hart_id = 0`, misaligned accesses permitted on both the
//!   hart and the bus. `_start` sets `sp` and `gp` itself.
//!
//! # What this does NOT reproduce — the M3↔M7 cross-check's seed
//!
//! Written down here because M7 boots the same image from reset through the
//! real ROM and the real bootloader, and every line below is a place the two
//! paths can disagree:
//!
//! 1. **The partition table is never read or validated.** No
//!    `esp_app_desc` check, no image-hash check, no secure-boot or
//!    flash-encryption path. A corrupt image boots here and does not there.
//! 2. **The MMU page table is programmed by the loader, not by a
//!    bootloader** ([`stage_image_in_flash`], M4). The bytes go into the
//!    flash chip at `factory + (vaddr - 0x4200_0000)` and the table maps
//!    them back, so the window really does read through the MMU — but the
//!    flash offsets are the loader's arithmetic, not an `esptool` image's
//!    segment layout, and no image header, hash or partition table was
//!    involved in choosing them. Likewise the flash **chip size** is
//!    written into the ROM's legacy chip struct by
//!    [`seed_rom_flash_chip`] in place of the bootloader's
//!    `esp_rom_spiflash_config_param` call.
//! 3. **The ROM's console is never initialised.** `uartAttach`,
//!    `ets_install_uart_printf`, the printf-channel selection and every ROM
//!    global they set are untouched, so `ets_get_printf_channel` answers from
//!    an unwritten ROM data segment. P6 and M7 care.
//! 4. **No `rst:0x1 (POWERON)` banner and no bootloader log.** The
//!    second-stage bootloader's own output is a large part of what a boot
//!    transcript compares, and none of it exists on this path.
//! 5. **No early RNG entropy.** The ROM stirs the RNG during boot; here the
//!    machine's seeded PRNG stands in, which is deterministic by design
//!    (PD5) and therefore not what silicon had.
//! 6. **eFuse is asserted, not read from the chip.** The MAC and wafer
//!    revision come from [`EfuseIdentity`], defaulting to the desk board.
//! 7. **The reset cause is asserted as POWERON.** Nothing consulted a PMU
//!    register to decide it.

use lp_emu_esp_common::{ElfImage, SocBus};
use lp_riscv_emu::mach::{MachineHart, csr};

use crate::cache::CacheHandle;
use crate::flash::{FACTORY_OFFSET, FlashHandle};
use crate::memmap;
use crate::rom::{RomError, place_spanning};

/// The desk board's MAC — `A0:F2:62:87:B4:8C`, the C6 on
/// `/dev/cu.usbmodem1433201`. The default so that a run with no `--efuse-*`
/// flags produces the same identity as the board every transcript came from.
pub const DESK_MAC: [u8; 6] = [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c];

/// What the eFuse block reports. P5 wires it to the EFUSE peripheral's reads;
/// P4 carries it so the identity is decided in one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EfuseIdentity {
    pub mac: [u8; 6],
    /// Wafer version major/minor — the desk board is v0.2.
    pub wafer_major: u8,
    pub wafer_minor: u8,
}

impl Default for EfuseIdentity {
    fn default() -> Self {
        Self {
            mac: DESK_MAC,
            wafer_major: 0,
            wafer_minor: 2,
        }
    }
}

impl EfuseIdentity {
    /// Parse `a0:f2:62:87:b4:8c`.
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

    /// Parse `0.2` into (major, minor).
    pub fn parse_rev(text: &str) -> Result<(u8, u8), String> {
        let (major, minor) = text
            .split_once('.')
            .ok_or_else(|| format!("`{text}` is not `<major>.<minor>`"))?;
        Ok((
            major.parse().map_err(|e| format!("`{major}`: {e}"))?,
            minor.parse().map_err(|e| format!("`{minor}`: {e}"))?,
        ))
    }
}

/// The reset cause the machine asserts. Direct load is always
/// [`ResetCause::PowerOn`]; M7 derives it from PMU registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetCause {
    PowerOn,
}

impl ResetCause {
    /// The value the ROM's `rtc_get_reset_reason` returns for this cause.
    pub const fn rom_code(self) -> u32 {
        match self {
            // `POWERON_RESET` — the value `__pre_init` compares against 1
            // before zeroing `.rtc_fast.persistent`.
            ResetCause::PowerOn => 1,
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

/// Something wrong with the app image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Rom(RomError),
    /// The entry point is not inside any `PT_LOAD`.
    ///
    /// `entry != 0` is **not** a usable check in this repository: the rv32
    /// guest images under `lp-emu/` link at zero on purpose. "The entry lies
    /// in a segment we placed" is the check that means something.
    EntryNotLoadable {
        entry: u32,
    },
    /// No loadable segment at all.
    NothingToLoad,
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
    let loadable: Vec<_> = app.segments.iter().filter(|s| s.memsz > 0).collect();
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
            // Not an error — a note. On ESP images `.rtc_fast.data` links to
            // RTC RAM with its load address in flash, and a loader that
            // silently used one for the other is how it ends up in the wrong
            // place. The shipped C6 image has none of these.
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

/// Put the hart where the bootloader would have left it.
///
/// `mstatus = 0x1888` is the load-bearing line: `MPP = 3`, `MPIE = 1` and —
/// the point — **`MIE = 1`**. Discovery §1h found no site in esp-hal 1.1.1,
/// esp-rtos, esp-sync or esp-riscv-rt that ever sets it, so a hart at the
/// architectural reset value never delivers an interrupt and the firmware
/// idles in `wfi` forever.
pub fn reset_hart(hart: &mut MachineHart<SocBus>, bus: &mut SocBus, entry: u32) {
    *hart = MachineHart::new(0);
    hart.set_pc(entry);
    assert!(hart.set_csr_raw(csr::MSTATUS, csr::MSTATUS_BOOT));
    assert!(hart.set_csr_raw(csr::MTVEC, 0));
    assert!(hart.set_csr_raw(csr::MIE, 0));

    // The C6 core performs misaligned data accesses in hardware. The hart's
    // flag only warns; the bus is what enforces, so both are set (P2's report
    // called this out and P4 is where it lands).
    hart.set_allow_unaligned(true);
    bus.set_allow_unaligned(true);
}

/// One page a direct load put into flash and mapped into the cache window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedPage {
    /// Where the guest sees it: `0x4200_0000 + n * 64 KiB`.
    pub vaddr: u32,
    /// Where it lives in the flash image.
    pub paddr: u32,
}

/// What the direct load put into flash.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlashStaging {
    /// Every page written and mapped, in address order.
    pub pages: Vec<StagedPage>,
    /// Bytes copied out of the ELF (not counting the zero tails).
    pub bytes: u32,
    /// The chip size written into `rom_spiflash_legacy_data`.
    pub chip_size: u32,
}

/// Put the flash-resident half of an image into the flash chip and program
/// the cache MMU for it — the two things the second-stage bootloader does
/// that a direct load otherwise skips.
///
/// # Why this exists at all
///
/// M3's direct load placed the ELF's `0x4200_0000` segments straight into
/// the RAM region behind the window and left the MMU empty
/// (`the module docs, item 2`). That works right up until something asks the
/// flash *chip* a question, and this milestone's firmware does: littlefs
/// mounts `lpfs` at `0x0031_0000` through the mask ROM's
/// `esp_rom_spiflash_read`, which reads the same part the app's `.text`
/// lives in. So the chip has to hold the app too, or the two halves of the
/// address space would be describing different boards.
///
/// # The mapping, and why it is this one
///
/// `paddr = 0x0001_0000 + (vaddr - 0x4200_0000)` — the `factory` partition's
/// offset (`lp-fw/fw-esp32c6/partitions.csv`) plus the offset into the
/// window. `0x0001_0000` is a whole number of 64 KiB pages, so
/// `paddr % 64K == vaddr % 64K` holds for every page, which is the cache
/// MMU's constraint and the reason esp-hal links at `0x4200_0020`.
///
/// It is **not** what an `esptool` image would produce: a real flashed image
/// has a header and per-segment headers, and its flash offsets fall where
/// those leave them. This is the direct-load equivalent of what a flasher
/// did — the bytes are in `factory`, at page-consistent offsets, and the
/// table says where. M7 boots a real merged image through the real
/// bootloader and gets the real offsets; that is the cross-check, and this
/// function is one of the things it checks.
///
/// The ROM's own `0x4200_0000` segment is staged first and the app's on top,
/// in the same order [`load_app`] places them into the window, so the flash
/// image and the window agree byte for byte.
pub fn stage_image_in_flash(
    flash: &FlashHandle,
    mmu: &CacheHandle,
    images: &[&ElfImage],
) -> FlashStaging {
    let page_len = mmu.lock().unwrap().page_len();
    let mut staging = FlashStaging::default();
    let mut touched: Vec<u32> = Vec::new();

    for image in images {
        for seg in &image.segments {
            if seg.memsz == 0 {
                continue;
            }
            let Some(offset) = seg.vaddr.checked_sub(memmap::FLASH_CACHE_BASE) else {
                continue;
            };
            if offset >= crate::cache::WINDOW_LEN {
                continue;
            }
            let paddr = FACTORY_OFFSET + offset;
            let mut chip = flash.lock().unwrap();
            if !seg.data.is_empty() && chip.stage(paddr, &seg.data) {
                staging.bytes += seg.data.len() as u32;
            }
            // The `memsz - filesz` tail is zeroed in the window, so it must
            // be zeroed in flash too — erased flash is `0xff`, and a `.bss`
            // that read as ones would not be a `.bss`.
            let tail = seg.memsz.saturating_sub(seg.data.len() as u32);
            if tail > 0 {
                let zeros = vec![0u8; tail as usize];
                chip.stage(paddr + seg.data.len() as u32, &zeros);
            }
            drop(chip);

            let first = offset / page_len;
            let last = (offset + seg.memsz - 1) / page_len;
            for page in first..=last {
                if !touched.contains(&page) {
                    touched.push(page);
                }
            }
        }
    }

    touched.sort_unstable();
    let mut mmu = mmu.lock().unwrap();
    for page in touched {
        let vaddr = memmap::FLASH_CACHE_BASE + page * page_len;
        let paddr = FACTORY_OFFSET + page * page_len;
        if mmu.map(vaddr, paddr) {
            staging.pages.push(StagedPage { vaddr, paddr });
        } else {
            log::warn!("loader: could not map {vaddr:#010x} to flash {paddr:#010x}");
        }
    }
    staging.chip_size = flash.lock().unwrap().len();
    staging
}

/// Tell the mask ROM how big the flash chip is, the way the second-stage
/// bootloader does.
///
/// `rom_spiflash_legacy_data` (`0x4087_ffec`) is a **pointer** to the chip
/// description, and the ROM's startup data seeds it to
/// `rom_default_spiflash_legacy_data` at `0x4087_fa08`, whose
/// `chip_size` word is `0x0020_0000` — **2 MiB**, the ROM's default part.
/// `SPI_read_data` (`0x4002_4100`) refuses any read past it:
///
/// ```text
/// c.lw  a5, 4(a0)          ; chip->chip_size
/// add   a4, a3, a1         ; len + addr
/// bltu  a5, a4, +0xae      ; → return 1
/// ```
///
/// and `lpfs` starts at `0x0031_0000`, which is past 2 MiB. On silicon the
/// bootloader calls `esp_rom_spiflash_config_param` with the size from the
/// image header's flash-size field; here the loader writes the same word,
/// derived from the image the machine was actually given. Without it every
/// littlefs read returns error 1 and the mount fails for a reason that has
/// nothing to do with the filesystem.
///
/// Returns the pointer it followed and the size it wrote, or `None` if the
/// ROM data was not seeded (which would mean the ROM ELF changed shape).
pub fn seed_rom_flash_chip(bus: &mut SocBus, chip_size: u32) -> Option<(u32, u32)> {
    let mut word = [0u8; 4];
    read_bytes(bus, memmap::ROM_SPIFLASH_LEGACY_DATA, &mut word)?;
    let chip = u32::from_le_bytes(word);
    if chip == 0 {
        log::warn!(
            "loader: rom_spiflash_legacy_data at {:#010x} is null; the ROM's initialised data \
             was not seeded and the flash chip description does not exist",
            memmap::ROM_SPIFLASH_LEGACY_DATA
        );
        return None;
    }
    // `chip_size` is the second word of the struct (`SPI_read_data` reads it
    // at `+4`); `device_id` is the first and stays the ROM's own.
    bus.load_image(chip + 4, &chip_size.to_le_bytes()).ok()?;
    Some((chip, chip_size))
}

/// Read `out.len()` bytes out of a RAM region, or `None` if the address is
/// not in one. (`SocBus` has `load_image` for the other direction; this is
/// the small counterpart the loader needs and nothing else does.)
fn read_bytes(bus: &SocBus, address: u32, out: &mut [u8]) -> Option<()> {
    for region in bus.regions() {
        if region.contains(address) && region.contains(address + out.len() as u32 - 1) {
            let at = (address - region.base) as usize;
            out.copy_from_slice(&region.data[at..at + out.len()]);
            return Some(());
        }
    }
    None
}

/// `dram2_seg` after the bootloader has gone: 64 KiB of zeroed RAM.
///
/// Nothing is *seeded* — the point of the function is that it asserts the
/// bytes are RAM and are zero, which is what "the loader vacated" means and
/// what the second heap region needs. It runs after the app's segments so a
/// stale ROM `.bss` overlap cannot leave a non-zero byte behind.
pub fn clear_dram2(bus: &mut SocBus) -> Result<(), LoadError> {
    let len = memmap::DRAM2_END - memmap::DRAM2_BASE;
    let zeros = vec![0u8; len as usize];
    place_spanning(bus, memmap::DRAM2_BASE, &zeros, len)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_and_a_revision_parse_the_way_the_flags_spell_them() {
        assert_eq!(
            EfuseIdentity::parse_mac("a0:f2:62:87:b4:8c").unwrap(),
            DESK_MAC
        );
        assert!(EfuseIdentity::parse_mac("a0-f2-62-87-b4-8c").is_err());
        assert!(EfuseIdentity::parse_mac("a0:f2:62:87:b4").is_err());
        assert!(EfuseIdentity::parse_mac("zz:f2:62:87:b4:8c").is_err());
        assert_eq!(EfuseIdentity::parse_rev("0.2").unwrap(), (0, 2));
        assert!(EfuseIdentity::parse_rev("02").is_err());

        let d = EfuseIdentity::default();
        assert_eq!((d.mac, d.wafer_major, d.wafer_minor), (DESK_MAC, 0, 2));
    }

    #[test]
    fn power_on_is_the_reason_code_pre_init_compares_against() {
        // `__pre_init` zeroes `.rtc_fast.persistent` iff the ROM returns 1.
        assert_eq!(ResetCause::PowerOn.rom_code(), 1);
    }
}
