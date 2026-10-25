//! Layout 2: the split image in `factory` with its region at `+0x10000`, on
//! 64 KiB MMU pages — the ESP32-S3's.
//!
//! ```text
//! factory +0x0000  (0x10000)  loader image          (layout 1's, LOADER_MAX_LEN)
//! factory +0x5000  (0x15000)  progress record       (layout 1's)
//! factory +0x6000  (0x16000)  boot record, sector 0 (layout 1's)
//! factory +0x7000  (0x17000)  boot record, sector 1 (layout 1's)
//! factory +0x8000  (0x18000)  ┐ reserved: erased, never written
//!                             ┘ (the price of the 64 KiB page)
//! factory +0x10000 (0x20000)  ┐ the region: one core (low or high end) and
//!   …                         │ one engine, by layout 1's rules
//! factory end                 ┘ (read from the flashed partition table)
//! ```
//!
//! # Why the region moves
//!
//! The S3's flash MMU has **one** page size, 64 KiB: the mask ROM's
//! `Cache_Ibus_MMU_Set` refuses any other (`bnei a5, 64`, modelled in
//! `lp-emu-esp32s3`'s `cache.rs`). A flash page maps only to a virtual page
//! at the same offset within a page, so a core whose segments must be
//! page-congruent cannot start at layout 1's [`REGION_START`] (`0x18000`,
//! 32 KiB-aligned only). The region's start moves to the next 64 KiB page;
//! nothing else does.
//!
//! # What is layout 1's
//!
//! Everything below [`REGION_START`] is layout 1's bytes at layout 1's
//! offsets, re-exported here and never re-typed: the loader's start and
//! limit, the progress record, the two boot-record sectors. So
//! [`crate::BootRecord`], its marks, [`crate::choose`],
//! [`crate::EngineHeader`], the progress record and
//! [`crate::engine_digest`] are unchanged, and one [`crate::SplitLayout`]
//! ([`crate::SplitLayout::layout_2`]) carries the region's rules.
//!
//! # The reserved gap
//!
//! [`LAYOUT_2_RESERVED`] (`0x18000..0x20000`, where layout 1's region
//! began) holds nothing. Nothing writes it: not the packager, not the update
//! session, not the loader. Giving it to the loader instead would make
//! [`LOADER_MAX_LEN`] a per-layout number; reserving it keeps it one.
//!
//! These offsets bind from the first release whose S3 core can install an
//! update, as layout 1's did (the S3 plan's ruling L3).

use core::ops::Range;

pub use crate::split_layout::{
    BOOT_RECORD_SECTORS, LOADER_MAX_LEN, LOADER_OFFSET, PROGRESS_RECORD_SECTOR, REGION_START,
};

/// Layout 2's MMU page: the S3's only one.
pub const LAYOUT_2_PAGE: u32 = 0x1_0000;
/// Layout 2's region: the first 64 KiB page after the boot records
/// (`factory + 0x10000`), and the low end's core offset.
pub const LAYOUT_2_REGION_START: u32 = 0x2_0000;
/// Erased, never written: from where layout 1's region began to where
/// layout 2's does.
pub const LAYOUT_2_RESERVED: Range<u32> = REGION_START..LAYOUT_2_REGION_START;

// Below the gap is layout 1; the region starts on the page, right after it.
const _: () = {
    assert!(BOOT_RECORD_SECTORS[1] + 0x1000 == LAYOUT_2_RESERVED.start);
    assert!(LAYOUT_2_RESERVED.end == LAYOUT_2_REGION_START);
    assert!(LAYOUT_2_REGION_START % LAYOUT_2_PAGE == 0);
    assert!(LAYOUT_2_REGION_START == LOADER_OFFSET + LAYOUT_2_PAGE);
};

#[cfg(test)]
mod tests {
    use crate::SplitLayout;

    use super::*;

    #[test]
    fn todays_factory_and_the_planned_slot_both_lay_out() {
        // Today's 6 MB factory, and a 3.5 MB one.
        let today = SplitLayout::layout_2(0x1_0000, 0x60_0000).unwrap();
        assert_eq!(today.region_start, LAYOUT_2_REGION_START);
        assert_eq!(today.region_end, 0x61_0000);
        assert_eq!(today.page, LAYOUT_2_PAGE);
        let room = today.engine_room(LAYOUT_2_REGION_START, 0x10_0001);
        assert_eq!(room.start, 0x13_0000, "the first page after the core");
        assert!(SplitLayout::layout_2(0x1_0000, 0x38_0000).is_some());
        assert!(SplitLayout::layout_2(0x1_8000, 0x38_0000).is_none());
        assert!(SplitLayout::layout_2(0x1_0000, 0x38_8000).is_none());
    }

    #[test]
    fn a_core_goes_high_then_low_again() {
        let l = SplitLayout::layout_2(0x1_0000, 0x60_0000).unwrap();
        let high = l
            .next_core_offset(LAYOUT_2_REGION_START, 0x10_0000, 0x10_8000)
            .unwrap();
        assert_eq!(high, 0x50_0000);
        assert!(!l.core_is_low(high));
        assert_eq!(l.engine_room(high, 0x10_8000).start, LAYOUT_2_REGION_START);
        assert_eq!(
            l.next_core_offset(high, 0x10_8000, 0x10_0000),
            Some(LAYOUT_2_REGION_START)
        );
    }

    #[test]
    fn below_the_gap_is_layout_one() {
        assert_eq!(LOADER_OFFSET, 0x1_0000);
        assert_eq!(PROGRESS_RECORD_SECTOR, LOADER_OFFSET + 0x5000);
        assert_eq!(
            BOOT_RECORD_SECTORS,
            [LOADER_OFFSET + 0x6000, LOADER_OFFSET + 0x7000]
        );
        assert_eq!(LAYOUT_2_RESERVED, 0x1_8000..0x2_0000);
        assert_eq!(LAYOUT_2_REGION_START, LOADER_OFFSET + 0x1_0000);
    }

    #[test]
    fn nothing_in_the_region_touches_the_gap() {
        let l = SplitLayout::layout_2(0x1_0000, 0x60_0000).unwrap();
        assert!(!l.fits(LAYOUT_2_RESERVED.start, 0x1000));
        assert!(!l.fits(LAYOUT_2_RESERVED.start, LAYOUT_2_PAGE));
        assert!(l.fits(LAYOUT_2_REGION_START, LAYOUT_2_PAGE));
        let high = l
            .next_core_offset(LAYOUT_2_REGION_START, 0x10_0000, 0x10_0000)
            .unwrap();
        assert!(l.engine_room(high, 0x10_0000).start >= LAYOUT_2_RESERVED.end);
    }

    #[test]
    fn every_extent_is_on_a_64k_page() {
        let l = SplitLayout::layout_2(0x1_0000, 0x60_0000).unwrap();
        assert!(!l.fits(LAYOUT_2_REGION_START + 0x8000, 0x1000));
        for core_len in [1, 0xFFFF, 0x1_0000, 0x1_0001, 0x12_3456] {
            let room = l.engine_room(LAYOUT_2_REGION_START, core_len);
            assert_eq!(room.start % LAYOUT_2_PAGE, 0, "{core_len:#x}");
            let high = l
                .next_core_offset(LAYOUT_2_REGION_START, core_len, core_len)
                .unwrap();
            assert_eq!(high % LAYOUT_2_PAGE, 0, "{core_len:#x}");
        }
    }

    #[test]
    fn a_factory_too_small_or_misplaced_is_refused() {
        assert!(SplitLayout::layout_2(0x1_0000, 0x1_0000).is_none());
        assert!(SplitLayout::layout_2(0x1_0000, 0x2_0000).is_some());
        assert!(SplitLayout::layout_2(0x2_0000, 0x60_0000).is_none());
        assert!(SplitLayout::layout_2(0x1_0000, u32::MAX).is_none());
    }

    #[test]
    fn layout_one_refuses_the_page_layout_two_needs() {
        assert_eq!(
            SplitLayout::from_factory(0x1_0000, 0x60_0000, LAYOUT_2_PAGE),
            None
        );
        let one = SplitLayout::from_factory(0x1_0000, 0x60_0000, 0x8000).unwrap();
        let two = SplitLayout::layout_2(0x1_0000, 0x60_0000).unwrap();
        assert_ne!(one.region_start, two.region_start);
    }
}
