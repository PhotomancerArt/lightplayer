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
//! 2. **No MMU page table.** The flash cache window is flat RAM holding the
//!    ELF's flash-mapped segments; the real bootloader programs the cache
//!    MMU, which is why esp-hal's link base is `0x4200_0020` and not
//!    `0x4200_0000`. M4 replaces it.
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
