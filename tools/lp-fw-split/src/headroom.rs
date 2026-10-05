//! What room the split image leaves, by every definition that can bind.
//!
//! The split costs almost nothing in code but up to two MMU pages of image
//! (the core and the engine each start on a page), and its room is
//! two-placement constrained: the core alternates between the region's low
//! and high ends with each update. So "headroom" is several numbers, and
//! each says which it is:
//!
//! - **image**: `factory` − `app.bin` — what the old single number became;
//! - **steady, low**: the region − the page-rounded core − the engine, with
//!   the core at the low end (as flashed);
//! - **steady, high**: the same with the core at the high end (after an
//!   update), where the engine fills the room before it;
//! - **update**: whether a second core of the same size fits beside the
//!   running one — the room an update needs while both exist;
//! - **legacy overlap**: from `app.bin`'s end to `0x310000`, where the
//!   pre-repartition `lpfs` began (informational, never a gate).
//!
//! The gate is the smallest of the steady and update headrooms.

use lp_bootctl::{LOADER_OFFSET, REGION_START, SplitLayout};

use crate::split_build::SplitReport;

/// Where the pre-2026-10 `lpfs` began (`lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv`).
pub const LEGACY_LPFS_OFFSET: u32 = 0x31_0000;

/// The headroom numbers, in bytes (negative: over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Headroom {
    pub factory_len: u32,
    pub app_bin_len: u32,
    pub image: i64,
    pub steady_low: i64,
    pub steady_high: i64,
    pub update: i64,
    pub legacy_overlap: i64,
}

impl Headroom {
    /// The numbers for a split image in a `factory` of `factory_len` bytes.
    pub fn of(report: &SplitReport, factory_len: u32) -> Self {
        let layout = SplitLayout {
            region_end: LOADER_OFFSET + factory_len,
            page: report.page,
        };
        let page = i64::from(report.page);
        let up = |x: i64| (x + page - 1) / page * page;
        let down = |x: i64| x / page * page;
        let region_start = i64::from(REGION_START);
        let region_end = i64::from(layout.region_end);
        let core = i64::from(report.core.size_bytes);
        let engine = i64::from(report.engine.size_bytes);
        let low_core_end = up(region_start + core);
        let high_core_at = down(region_end - core);
        let app_bin_len = report.app_bin.size_bytes;
        Self {
            factory_len,
            app_bin_len,
            image: i64::from(factory_len) - i64::from(app_bin_len),
            steady_low: region_end - low_core_end - engine,
            steady_high: (high_core_at - region_start) - engine,
            update: high_core_at - low_core_end,
            legacy_overlap: i64::from(LEGACY_LPFS_OFFSET)
                - (i64::from(LOADER_OFFSET) + i64::from(app_bin_len)),
        }
    }

    /// The number the gate holds against the margin.
    pub fn gated(&self) -> i64 {
        self.steady_low.min(self.steady_high).min(self.update)
    }

    /// The report, one number per line.
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!(
                "split image: app.bin {} B / factory {} B — image headroom {} B",
                self.app_bin_len, self.factory_len, self.image
            ),
            format!(
                "split headroom, steady (core low, as flashed): {} B",
                self.steady_low
            ),
            format!(
                "split headroom, steady (core high, after an update): {} B",
                self.steady_high
            ),
            format!(
                "split headroom, update (a second core of the same size): {} B",
                self.update
            ),
            format!(
                "legacy overlap: {} B before app.bin reaches the old lpfs at {LEGACY_LPFS_OFFSET:#x}",
                self.legacy_overlap
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split_build::{Piece, SplitReport};

    fn report(core: u32, engine: u32) -> SplitReport {
        let piece = |offset, size_bytes| Piece {
            offset,
            size_bytes,
            sha256: String::new(),
        };
        let engine_off = (REGION_START + core).div_ceil(0x8000) * 0x8000;
        SplitReport {
            layout: 1,
            page: 0x8000,
            region_end: 0x35_0000,
            app_version: String::new(),
            build_id: String::new(),
            loader_version: 1,
            loader: piece(LOADER_OFFSET, 2800),
            core: piece(REGION_START, core),
            engine: piece(engine_off, engine),
            app_bin: piece(LOADER_OFFSET, engine_off + engine - LOADER_OFFSET),
            merged_sha256: String::new(),
            room_left: 0,
            pass1_seconds: 0.0,
            pass2_seconds: 0.0,
            core_in_engine: 0,
            core_to_engine_edges: 0,
        }
    }

    #[test]
    fn the_numbers_follow_their_definitions() {
        // core 1,161,104 B, engine 1,824,152 B (the P04 image).
        let h = Headroom::of(&report(1_161_104, 1_824_152), 0x34_0000);
        let engine_off = 0x13_8000i64;
        assert_eq!(h.steady_low, 0x35_0000 - engine_off - 1_824_152);
        let high = (0x35_0000i64 - 1_161_104) / 0x8000 * 0x8000;
        assert_eq!(h.steady_high, high - 0x1_8000 - 1_824_152);
        assert_eq!(h.update, high - engine_off);
        assert_eq!(h.image, 0x34_0000 - (engine_off + 1_824_152 - 0x1_0000));
        assert_eq!(h.gated(), h.steady_low.min(h.steady_high).min(h.update));
    }

    #[test]
    fn an_image_too_big_goes_negative() {
        let h = Headroom::of(&report(1_700_000, 1_700_000), 0x34_0000);
        assert!(h.update < 0);
        assert!(h.gated() < 0);
    }
}
