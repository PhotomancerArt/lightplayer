//! Where a split-link image's pieces live inside the one app partition,
//! `factory` (layout 1).
//!
//! ```text
//! factory +0x0000  (0x10000)  loader image          (what the IDF bootloader boots)
//! factory +0x5000  (0x15000)  reserved: an over-the-air update's progress record
//! factory +0x6000  (0x16000)  boot record, sector 0
//! factory +0x7000  (0x17000)  boot record, sector 1
//! factory +0x8000  (0x18000)  ┐
//!   …                         │ the region: one core image and one engine.
//! factory end                 ┘ The core sits at the LOW end (right here) or
//!                               the HIGH end (page-aligned against the end),
//!                               alternating with each update; the engine
//!                               fills whatever the core does not.
//! ```
//!
//! The fixed starts are constants. The region's **end** is `factory`'s end
//! as read from the flashed partition table ([`SplitLayout::from_factory`]),
//! never a constant: a board's table says where its `lpfs` begins.
//!
//! # The page
//!
//! Every extent starts on an MMU page boundary: a flash page maps only to a
//! virtual page at the same offset within a page, and both the core and the
//! engine are linked at fixed virtual addresses. The layout assumes a
//! **32 KiB** page, which is what the IDF bootloader the firmware ships with
//! (espflash 3.3.0's) selects on a 4 MB C6: [`REGION_START`] (`0x18000`) is
//! 32 KiB-aligned but **not** 64 KiB-aligned, so a bootloader that chose
//! 64 KiB pages cannot boot this layout. The loader and the core read the
//! page from the MMU rather than assume it, and refuse an extent that does
//! not fit it — a mismatch is refused, never misbooted.
//!
//! The update writes the new core into the end the current core is not in
//! (destroying the engine, which is erased first anyway), so a core can grow
//! until `old core + new core` no longer fit the region. The engine's room is
//! whatever the current core leaves.

/// Where the IDF bootloader finds the loader: the app partition's start.
pub const LOADER_OFFSET: u32 = 0x1_0000;
/// The loader image may not reach the progress-record sector.
pub const LOADER_MAX_LEN: u32 = 0x5000;
/// Reserved for an over-the-air update's progress record. Nothing in this
/// layout's first version writes it; it is named so nothing else grows into
/// it.
pub const PROGRESS_RECORD_SECTOR: u32 = 0x1_5000;
/// The two boot-record sectors.
pub const BOOT_RECORD_SECTORS: [u32; 2] = [0x1_6000, 0x1_7000];
/// The first byte of the region, and the low end's core offset.
pub const REGION_START: u32 = 0x1_8000;
/// The smallest MMU page the layout accepts.
const MIN_PAGE: u32 = 0x1000;

/// The region and the MMU page it must respect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitLayout {
    pub region_end: u32,
    pub page: u32,
}

/// A byte range of flash, `[start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub start: u32,
    pub end: u32,
}

impl Extent {
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SplitLayout {
    /// The layout inside a `factory` partition at `offset`, `len` bytes long,
    /// under an MMU page of `page` bytes. `None` when `factory` does not
    /// start where the bootloader finds the loader, when it cannot hold the
    /// loader, the records and one page of region, or when `page` is not a
    /// power of two of at least 4 KiB that `REGION_START` respects.
    pub fn from_factory(offset: u32, len: u32, page: u32) -> Option<Self> {
        if offset != LOADER_OFFSET || !page.is_power_of_two() || page < MIN_PAGE {
            return None;
        }
        if REGION_START % page != 0 {
            return None;
        }
        let region_end = offset.checked_add(len)?;
        if region_end < REGION_START.checked_add(page)? {
            return None;
        }
        Some(Self { region_end, page })
    }

    fn up(&self, x: u32) -> u32 {
        x.div_ceil(self.page) * self.page
    }

    fn down(&self, x: u32) -> u32 {
        x / self.page * self.page
    }

    /// The core sits at the low end of the region.
    pub fn core_is_low(&self, core_off: u32) -> bool {
        core_off == REGION_START
    }

    /// The most the engine may occupy for a core at `core_off`/`core_len`:
    /// from the first page after a low core to the region's end, or from the
    /// region's start to a high core.
    pub fn engine_room(&self, core_off: u32, core_len: u32) -> Extent {
        if self.core_is_low(core_off) {
            Extent {
                start: self.up(core_off + core_len),
                end: self.region_end,
            }
        } else {
            Extent {
                start: REGION_START,
                end: core_off,
            }
        }
    }

    /// Where a new core of `new_len` bytes goes while the core at
    /// `core_off`/`core_len` still runs: the other end, without overlap.
    pub fn next_core_offset(&self, core_off: u32, core_len: u32, new_len: u32) -> Option<u32> {
        if self.core_is_low(core_off) {
            let at = self.down(self.region_end.checked_sub(new_len)?);
            (at >= self.up(core_off + core_len)).then_some(at)
        } else {
            (REGION_START + new_len <= core_off).then_some(REGION_START)
        }
    }

    /// Whether an extent starts on a page boundary inside the region.
    pub fn fits(&self, start: u32, len: u32) -> bool {
        start % self.page == 0
            && start >= REGION_START
            && start
                .checked_add(len)
                .is_some_and(|end| end <= self.region_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: u32 = 0x8000;

    /// Today's table (2026-10 repartition): `factory` 0x10000 + 0x340000.
    fn today() -> SplitLayout {
        SplitLayout::from_factory(0x1_0000, 0x34_0000, PAGE).unwrap()
    }

    #[test]
    fn the_fixed_starts_share_the_page_boundaries() {
        assert_eq!(REGION_START % PAGE, 0);
        assert!(LOADER_OFFSET + LOADER_MAX_LEN <= PROGRESS_RECORD_SECTOR);
        assert!(PROGRESS_RECORD_SECTOR + 0x1000 <= BOOT_RECORD_SECTORS[0]);
        assert!(BOOT_RECORD_SECTORS[1] + 0x1000 <= REGION_START);
        assert_eq!(
            BOOT_RECORD_SECTORS,
            [LOADER_OFFSET + 0x6000, LOADER_OFFSET + 0x7000]
        );
        assert_eq!(REGION_START, LOADER_OFFSET + 0x8000);
    }

    #[test]
    fn todays_table_ends_the_region_at_0x350000() {
        assert_eq!(today().region_end, 0x35_0000);
    }

    #[test]
    fn the_legacy_table_ends_it_at_0x310000() {
        let legacy = SplitLayout::from_factory(0x1_0000, 0x30_0000, PAGE).unwrap();
        assert_eq!(legacy.region_end, 0x31_0000);
    }

    #[test]
    fn a_factory_elsewhere_is_refused() {
        assert_eq!(SplitLayout::from_factory(0x2_0000, 0x34_0000, PAGE), None);
    }

    #[test]
    fn a_factory_too_small_is_refused() {
        assert_eq!(SplitLayout::from_factory(0x1_0000, 0x8000, PAGE), None);
        assert!(SplitLayout::from_factory(0x1_0000, 0x1_0000, PAGE).is_some());
    }

    #[test]
    fn a_page_the_layout_cannot_respect_is_refused() {
        // 64 KiB: 0x18000 is not 64 KiB-aligned.
        assert_eq!(
            SplitLayout::from_factory(0x1_0000, 0x34_0000, 0x1_0000),
            None
        );
        assert_eq!(SplitLayout::from_factory(0x1_0000, 0x34_0000, 0x6000), None);
        assert_eq!(SplitLayout::from_factory(0x1_0000, 0x34_0000, 0x800), None);
        assert!(SplitLayout::from_factory(0x1_0000, 0x34_0000, 0x1000).is_some());
    }

    #[test]
    fn a_low_core_has_the_engine_after_it() {
        let e = today().engine_room(REGION_START, 1_150_816);
        assert_eq!(e.start % PAGE, 0);
        assert!(e.start >= REGION_START + 1_150_816);
        assert_eq!(e.end, 0x35_0000);
    }

    #[test]
    fn a_high_core_has_the_engine_before_it() {
        let l = today();
        let at = l
            .next_core_offset(REGION_START, 1_150_816, 1_160_000)
            .unwrap();
        assert_eq!(at % PAGE, 0);
        assert!(at + 1_160_000 <= 0x35_0000);
        assert_eq!(
            l.engine_room(at, 1_160_000),
            Extent {
                start: REGION_START,
                end: at
            }
        );
    }

    #[test]
    fn the_core_alternates_ends() {
        let l = today();
        let high = l
            .next_core_offset(REGION_START, 1_150_816, 1_150_816)
            .unwrap();
        assert!(!l.core_is_low(high));
        assert_eq!(
            l.next_core_offset(high, 1_150_816, 1_200_000),
            Some(REGION_START)
        );
    }

    #[test]
    fn two_cores_that_do_not_fit_together_are_refused() {
        let l = today();
        let region = l.region_end - REGION_START;
        assert_eq!(
            l.next_core_offset(REGION_START, region / 2 + PAGE, region / 2),
            None
        );
        let high = l
            .next_core_offset(REGION_START, 1_000_000, 1_000_000)
            .unwrap();
        assert_eq!(
            l.next_core_offset(high, 1_000_000, high - REGION_START + 1),
            None
        );
    }

    #[test]
    fn fits_respects_the_page_and_the_region() {
        let l = today();
        assert!(l.fits(REGION_START, 0x1000));
        assert!(!l.fits(REGION_START + 0x1000, 0x1000));
        assert!(!l.fits(0x34_8000, 0x9000));
        assert!(!l.fits(REGION_START, u32::MAX));
    }
}
