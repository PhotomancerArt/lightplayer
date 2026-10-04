//! Where a split-link image's pieces live inside the one app partition.
//!
//! ```text
//! 0x10000  loader image          (what the IDF bootloader boots)
//! 0x16000  boot record, sector 0
//! 0x17000  boot record, sector 1
//! 0x18000  ┐
//!   …      │ the region: one core image and one engine, laid out freely.
//!   end    ┘ The core sits at the LOW end (right here) or the HIGH end
//!            (page-aligned against `end`), alternating with each update;
//!            the engine fills whatever the core does not.
//! ```
//!
//! Every extent starts on an MMU page boundary: a flash page maps only to a
//! virtual page at the same offset within a page, and both the core and the
//! engine are linked at fixed virtual addresses. The IDF bootloader the
//! firmware ships with selects 32 KiB pages on a 4 MB part; the loader and
//! the core both read the page size from the MMU rather than assume it, and
//! refuse an extent that does not fit it.
//!
//! The update writes the new core into the end the current core is not in
//! (destroying the engine, which is erased first anyway), so a core can grow
//! until `old core + new core` no longer fit the region. The engine's room is
//! whatever the current core leaves.

/// Where the IDF bootloader finds the loader: the app partition's start.
pub const LOADER_OFFSET: u32 = 0x1_0000;
/// The loader image may not reach the boot records.
pub const LOADER_MAX_LEN: u32 = 0x6000;
/// The two boot-record sectors.
pub const BOOT_RECORD_SECTORS: [u32; 2] = [0x1_6000, 0x1_7000];
/// The first byte of the region, and the low end's core offset.
pub const REGION_START: u32 = 0x1_8000;
/// The C6's 4 MB layout today: `factory` ends where `lpfs` starts
/// (`lp-fw/fw-esp32c6/partitions.csv`, `factory` 0x10000 + 0x340000).
// Temporary: replaced by the end read from the flashed partition table.
pub const REGION_END_C6_4MB: u32 = 0x35_0000;

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
    pub const fn c6_4mb(page: u32) -> Self {
        Self {
            region_end: REGION_END_C6_4MB,
            page,
        }
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

    /// Where the engine lives for a core at `core_off`: after a low core, or
    /// before a high one.
    pub fn engine_extent(&self, core_off: u32, core_len: u32) -> Extent {
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
        start % self.page == 0 && start >= REGION_START && start + len <= self.region_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: u32 = 0x8000;
    const L: SplitLayout = SplitLayout::c6_4mb(PAGE);

    #[test]
    fn the_records_and_region_start_share_the_loaders_page_boundaries() {
        assert_eq!(REGION_START % PAGE, 0);
        assert!(LOADER_OFFSET + LOADER_MAX_LEN <= BOOT_RECORD_SECTORS[0]);
        assert!(BOOT_RECORD_SECTORS[1] + 0x1000 <= REGION_START);
    }

    #[test]
    fn a_low_core_has_the_engine_after_it() {
        let e = L.engine_extent(REGION_START, 1_150_816);
        assert_eq!(e.start % PAGE, 0);
        assert!(e.start >= REGION_START + 1_150_816);
        assert_eq!(e.end, REGION_END_C6_4MB);
    }

    #[test]
    fn a_high_core_has_the_engine_before_it() {
        let at = L
            .next_core_offset(REGION_START, 1_150_816, 1_160_000)
            .unwrap();
        assert_eq!(at % PAGE, 0);
        assert!(at + 1_160_000 <= REGION_END_C6_4MB);
        let e = L.engine_extent(at, 1_160_000);
        assert_eq!(
            e,
            Extent {
                start: REGION_START,
                end: at
            }
        );
    }

    #[test]
    fn the_core_alternates_ends() {
        let high = L
            .next_core_offset(REGION_START, 1_150_816, 1_150_816)
            .unwrap();
        assert!(!L.core_is_low(high));
        assert_eq!(
            L.next_core_offset(high, 1_150_816, 1_200_000),
            Some(REGION_START)
        );
    }

    #[test]
    fn two_cores_that_do_not_fit_together_are_refused() {
        let region = REGION_END_C6_4MB - REGION_START;
        assert_eq!(
            L.next_core_offset(REGION_START, region / 2 + PAGE, region / 2),
            None
        );
        let high = L
            .next_core_offset(REGION_START, 1_000_000, 1_000_000)
            .unwrap();
        assert_eq!(
            L.next_core_offset(high, 1_000_000, high - REGION_START + 1),
            None
        );
    }
}
