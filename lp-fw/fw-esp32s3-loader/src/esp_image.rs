//! Loading one ESP application image the way the IDF bootloader does, minus
//! its verification: the core's integrity is the trial mechanism's job (a
//! core that does not come up never confirms, and the next boot rolls back).
//!
//! Format: a 24-byte header (`0xE9`, segment count at byte 1, entry point at
//! bytes 4..8), then per segment an 8-byte header (load address, length) and
//! its bytes.
//!
//! # Where a segment may go
//!
//! - **Flash, data bus** (`0x3C00_0000..`) and **flash, instruction bus**
//!   (`0x4200_0000..`): mapped, one table for both, entry
//!   `(vaddr − bus) >> 16` (`lp_bootctl::s3_mmu`). The segment's flash bytes
//!   must sit at the same offset within a 64 KiB page as its link address,
//!   and it must stay below the loader's scratch window
//!   (`s3_mmu::CORE_ENTRY_LIMIT`, ruling L4).
//! - **RAM**: copied from flash. Data RAM, RTC RAM and SRAM1's instruction
//!   view (through its data-bus alias, [`lp_bootctl::SRAM1_I_FROM_D`]) take
//!   any store; SRAM0's instruction RAM has no data-bus alias and takes only
//!   32-bit stores. A copy may not reach either boot stage's RAM
//!   ([`lp_bootctl::S3_BOOT_STAGE_RAM`]): the loader's, which is running, or
//!   the IDF bootloader's, whose stack the loader runs on.
//! - **Anything else** is refused by name, address 0 first: espflash's image
//!   builder fills a gap before a flash segment with an address-0 segment,
//!   which the IDF bootloader skips and a copy would write to address 0
//!   (`docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md`).

use core::ffi::CStr;
use core::ops::Range;

use lp_bootctl::s3_mmu::{CORE_ENTRY_LIMIT, DROM_BASE, IROM_BASE, MMU_ENTRIES, MMU_PAGE};
use lp_bootctl::{S3_IDF_BOOTLOADER_RAM, S3_LOADER_RAM, SRAM1_I_FROM_D};

use crate::flash_window::FlashWindow;
use crate::mmu;

const MAGIC: u8 = 0xE9;
const HEADER_LEN: u32 = 24;
const MAX_SEGMENTS: u8 = 16;

/// The flash windows below the loader's scratch, each bus's.
const DROM: Range<u32> = DROM_BASE..DROM_BASE + CORE_ENTRY_LIMIT * MMU_PAGE;
const IROM: Range<u32> = IROM_BASE..IROM_BASE + CORE_ENTRY_LIMIT * MMU_PAGE;
/// Both buses' whole windows, scratch included: a segment here and not in
/// [`DROM`]/[`IROM`] reaches the scratch window.
const DBUS: Range<u32> = DROM_BASE..DROM_BASE + MMU_ENTRIES * MMU_PAGE;
const IBUS: Range<u32> = IROM_BASE..IROM_BASE + MMU_ENTRIES * MMU_PAGE;

/// SRAM0, instruction bus only: 32-bit stores.
const SRAM0_IRAM: Range<u32> = 0x4037_0000..0x4037_8000;
/// SRAM1's instruction view, copied through its data-bus alias.
const SRAM1_IRAM: Range<u32> = 0x4037_8000..0x403E_0000;
/// SRAM1's and SRAM2's data view.
const DRAM: Range<u32> = 0x3FC8_8000..0x3FD0_0000;
/// RTC fast memory (one address on both buses) and RTC slow memory.
const RTC_FAST: Range<u32> = 0x600F_E000..0x6010_0000;
const RTC_SLOW: Range<u32> = 0x5000_0000..0x5000_2000;

/// Map and load the image at flash `at`; its entry point.
pub fn load(flash: &FlashWindow, at: u32) -> Result<u32, &'static CStr> {
    let (Some(first), Some(entry)) = (flash.word(at), flash.word(at + 4)) else {
        return Err(c"flash read failed");
    };
    let bytes = first.to_le_bytes();
    let segments = bytes[1];
    if bytes[0] != MAGIC || segments == 0 || segments > MAX_SEGMENTS {
        return Err(c"no image there");
    }

    let mut off = at + HEADER_LEN;
    for _ in 0..segments {
        let (Some(addr), Some(len)) = (flash.word(off), flash.word(off + 4)) else {
            return Err(c"flash read failed");
        };
        let data = off + 8;
        let Some(end) = addr.checked_add(len) else {
            return Err(c"segment wraps");
        };
        if addr == 0 {
            return Err(c"address-0 segment");
        } else if let Some(bus) = flash_bus(addr, end) {
            map(bus, addr, end, data)?;
        } else if in_either_bus(addr) {
            return Err(c"segment in the scratch window");
        } else {
            copy(flash, addr, end, data)?;
        }
        off = data + len;
    }
    Ok(entry)
}

/// The bus window a flash segment `[addr, end)` lies in, below the scratch.
fn flash_bus(addr: u32, end: u32) -> Option<u32> {
    [DROM, IROM]
        .into_iter()
        .find(|w| addr >= w.start && end <= w.end)
        .map(|w| w.start)
}

fn in_either_bus(addr: u32) -> bool {
    DBUS.contains(&addr) || IBUS.contains(&addr)
}

/// Map every page of a flash segment at its link address.
fn map(bus: u32, addr: u32, end: u32, data: u32) -> Result<(), &'static CStr> {
    if data % MMU_PAGE != addr % MMU_PAGE {
        return Err(c"segment not page-congruent");
    }
    let mut v = addr - addr % MMU_PAGE;
    let mut p = data - data % MMU_PAGE;
    while v < end {
        mmu::map((v - bus) / MMU_PAGE, p);
        v += MMU_PAGE;
        p += MMU_PAGE;
    }
    Ok(())
}

/// Copy a RAM segment into place.
fn copy(flash: &FlashWindow, addr: u32, end: u32, data: u32) -> Result<(), &'static CStr> {
    let len = end - addr;
    let (dest, words_only) = if within(&SRAM0_IRAM, addr, end) {
        (addr, true)
    } else if within(&SRAM1_IRAM, addr, end) {
        (addr - SRAM1_I_FROM_D, false)
    } else if within(&DRAM, addr, end)
        || within(&RTC_FAST, addr, end)
        || within(&RTC_SLOW, addr, end)
    {
        (addr, false)
    } else {
        return Err(c"segment outside RAM");
    };
    // Through the data view, where both boot stages' RAM is named.
    if overlaps(&S3_LOADER_RAM, dest, dest + len) {
        return Err(c"segment overlaps the loader");
    }
    if overlaps(&S3_IDF_BOOTLOADER_RAM, dest, dest + len) {
        return Err(c"segment overlaps the IDF bootloader");
    }
    if words_only && !(dest | len | data).is_multiple_of(4) {
        return Err(c"instruction RAM segment not word-aligned");
    }
    // SAFETY: the core's RAM, free until it starts; checked against both
    // boot stages' RAM above.
    let copied = unsafe {
        if words_only {
            flash.copy_words(data, dest as *mut u32, len)
        } else {
            flash.copy(data, dest as *mut u8, len)
        }
    };
    copied.then_some(()).ok_or(c"flash read failed")
}

fn within(r: &Range<u32>, start: u32, end: u32) -> bool {
    start >= r.start && end <= r.end
}

fn overlaps(r: &Range<u32>, start: u32, end: u32) -> bool {
    start < r.end && end > r.start
}
