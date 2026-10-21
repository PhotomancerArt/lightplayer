//! SPIKE (`spike-s3-split-image`): the ESP32-S3's split-image layout, in
//! lp-bootctl terms. Not a product format — nothing reads it but the spike's
//! firmware and tool. Layout 1 ([`crate::SplitLayout`]) is untouched.
//!
//! ```text
//! factory +0x0000  (0x10000)  loader image          (the IDF bootloader boots it)
//! factory +0x5000  (0x15000)  progress record       (as layout 1)
//! factory +0x6000  (0x16000)  boot record, sector 0 (as layout 1)
//! factory +0x7000  (0x17000)  boot record, sector 1 (as layout 1)
//! factory +0x8000  (0x18000)  32 KiB unused         (the price of the 64 KiB page)
//! factory +0x10000 (0x20000)  ┐ the region: core low or high, engine between
//! factory end                 ┘
//! ```
//!
//! # Why the region moves
//!
//! The S3's flash MMU has **one** page size, 64 KiB (`Cache_Ibus_MMU_Set`
//! refuses any other: `bnei a5, 64`, lp-emu-esp32s3 `cache.rs`), so layout
//! 1's `REGION_START = 0x1_8000` cannot hold a core whose segments must be
//! page-congruent. Everything below the region is layout 1's bytes at layout
//! 1's offsets, so the records, the progress record and `choose` carry over
//! unchanged; only the region's start and the page differ.
//!
//! # The engine's two views
//!
//! The S3's MMU table serves the data bus (`0x3C00_0000 + n × 64 KiB`) and
//! the instruction bus (`0x4200_0000 + n × 64 KiB`) at the **same** index
//! `n`. The engine is one run of flash pages mapped once at
//! [`ENGINE_MMU_ENTRY`]: its header and rodata are read through
//! [`ENGINE_DROM`], its code fetched through [`ENGINE_IROM`] — the link puts
//! the code after the rodata at the same offset into the window, the trick
//! esp-hal's `.rotext_dummy` plays for the core.

use crate::split_layout::{BOOT_RECORD_SECTORS, Extent, LOADER_OFFSET};

/// The S3's only MMU page.
pub const S3_PAGE: u32 = 0x1_0000;
/// The region's first byte: the first 64 KiB page after the records.
pub const S3_REGION_START: u32 = 0x2_0000;
/// The MMU entry the engine's first page is mapped at.
pub const ENGINE_MMU_ENTRY: u32 = 0x100;
/// Where the engine's header and rodata are read (data bus, entry 0x100).
pub const ENGINE_DROM: u32 = 0x3C00_0000 + ENGINE_MMU_ENTRY * S3_PAGE;
/// Where the engine's code is fetched (instruction bus, the same entries).
pub const ENGINE_IROM: u32 = 0x4200_0000 + ENGINE_MMU_ENTRY * S3_PAGE;
/// The most the engine window holds: entries 0x100..0x1C0 (12 MiB).
pub const ENGINE_WINDOW: u32 = 0xC0 * S3_PAGE;
/// The loader's scratch window (entries 0x1C0..0x200), clear of both.
pub const SCRATCH_MMU_ENTRY: u32 = 0x1C0;

const _: () = {
    assert!(BOOT_RECORD_SECTORS[1] + 0x1000 <= S3_REGION_START);
    assert!(S3_REGION_START.is_multiple_of(S3_PAGE));
    assert!(ENGINE_MMU_ENTRY + ENGINE_WINDOW / S3_PAGE <= SCRATCH_MMU_ENTRY);
};

/// The S3 region, ending where `factory` ends in the flashed table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct S3SplitLayout {
    pub region_end: u32,
}

impl S3SplitLayout {
    /// `None` when `factory` does not start at the loader, or cannot hold one
    /// page of region, or does not end on a page.
    pub fn from_factory(offset: u32, len: u32) -> Option<Self> {
        let region_end = offset.checked_add(len)?;
        if offset != LOADER_OFFSET
            || !region_end.is_multiple_of(S3_PAGE)
            || region_end < S3_REGION_START + S3_PAGE
        {
            return None;
        }
        Some(Self { region_end })
    }

    fn up(x: u32) -> u32 {
        x.div_ceil(S3_PAGE) * S3_PAGE
    }

    /// The engine's room for a core at `core_off`/`core_len` (layout 1's rule
    /// on the S3's page).
    pub fn engine_room(&self, core_off: u32, core_len: u32) -> Extent {
        if core_off == S3_REGION_START {
            Extent {
                start: Self::up(core_off + core_len),
                end: self.region_end,
            }
        } else {
            Extent {
                start: S3_REGION_START,
                end: core_off,
            }
        }
    }

    /// Where a second core of `new_len` goes beside the running one.
    pub fn next_core_offset(&self, core_off: u32, core_len: u32, new_len: u32) -> Option<u32> {
        if core_off == S3_REGION_START {
            let at = self.region_end.checked_sub(new_len)? / S3_PAGE * S3_PAGE;
            (at >= Self::up(core_off + core_len)).then_some(at)
        } else {
            (S3_REGION_START + Self::up(new_len) <= core_off).then_some(S3_REGION_START)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todays_factory_and_the_planned_slot_both_lay_out() {
        // Today's 6 MB factory, and a 3.5 MB one.
        let today = S3SplitLayout::from_factory(0x1_0000, 0x60_0000).unwrap();
        assert_eq!(today.region_end, 0x61_0000);
        let room = today.engine_room(S3_REGION_START, 0x10_0001);
        assert_eq!(room.start, 0x13_0000, "the first page after the core");
        assert!(S3SplitLayout::from_factory(0x1_0000, 0x38_0000).is_some());
        assert!(S3SplitLayout::from_factory(0x1_8000, 0x38_0000).is_none());
        assert!(S3SplitLayout::from_factory(0x1_0000, 0x38_8000).is_none());
    }

    #[test]
    fn a_core_goes_high_then_low_again() {
        let l = S3SplitLayout::from_factory(0x1_0000, 0x60_0000).unwrap();
        let high = l
            .next_core_offset(S3_REGION_START, 0x10_0000, 0x10_8000)
            .unwrap();
        assert_eq!(high, 0x50_0000);
        assert_eq!(l.engine_room(high, 0x10_8000).start, S3_REGION_START);
        assert_eq!(
            l.next_core_offset(high, 0x10_8000, 0x10_0000),
            Some(S3_REGION_START)
        );
    }

    #[test]
    fn the_engine_views_share_one_mmu_entry() {
        assert_eq!(ENGINE_DROM, 0x3D00_0000);
        assert_eq!(ENGINE_IROM, 0x4300_0000);
        assert_eq!((ENGINE_DROM >> 16) & 0x1ff, (ENGINE_IROM >> 16) & 0x1ff);
    }
}
