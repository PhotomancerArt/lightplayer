//! What this boot is: which core is running, from which boot record, on
//! trial or not — and where that leaves room for the engine and the next core.
//!
//! Reads the same two records the loader read and makes the same choice
//! (`lp_bootctl::choose`), so the core and the loader agree by construction.

use lp_bootctl::{
    ATTEMPTED_MARK_OFFSET, BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootChoice, BootSlot,
    CONFIRMED_MARK_OFFSET, Extent, SplitLayout,
};

use super::engine_window::{page_size, running_core_offset};
use super::split_flash::SplitFlash;

pub struct BootState {
    pub layout: SplitLayout,
    pub choice: Option<BootChoice>,
    pub core_off: u32,
    pub core_len: u32,
    /// The build that failed its trial here, when the loader rolled back.
    pub failed_build: Option<u32>,
    /// Every read succeeded, the extent fits the layout, and the records
    /// agree with the MMU about where this core runs. Nothing is written —
    /// no mark, no update — unless this holds.
    pub healthy: bool,
}

impl BootState {
    /// A state that trusts nothing (what is left behind once the real one
    /// has been taken out of `CoreBoot`).
    pub fn placeholder() -> Self {
        Self {
            layout: SplitLayout::c6_4mb(page_size()),
            choice: None,
            core_off: 0,
            core_len: 0,
            failed_build: None,
            healthy: false,
        }
    }

    pub fn read(flash: &mut SplitFlash) -> Self {
        let layout = SplitLayout::c6_4mb(page_size());
        let mut sectors = [None; 2];
        let mut reads_ok = true;
        for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
            let mut buf = [0u8; BOOT_RECORD_READ_LEN];
            if flash.read(at, &mut buf) {
                *slot = BootSlot::decode(&buf);
            } else {
                reads_ok = false;
            }
        }
        let choice = lp_bootctl::choose(sectors, cold_boot());
        let failed_build = choice
            .filter(|c| c.rolled_back)
            .and_then(|c| sectors[1 - c.sector])
            .map(|s| s.record.build);
        let running = running_core_offset();
        let (core_off, core_len) = match choice {
            Some(c) => (c.slot.record.core_off, c.slot.record.core_len),
            // No record (a board flashed without one): the core is where the
            // MMU says it is; its length is the image's own.
            None => (running, image_len(flash, running).unwrap_or(0)),
        };
        let healthy =
            reads_ok && core_len > 0 && core_off == running && layout.fits(core_off, core_len);
        if !healthy {
            log::error!(
                "[OTA] boot state not trusted (reads {}, record core @{core_off:#x} +{core_len}, \
                 MMU says @{running:#x}) — updates refused this boot",
                if reads_ok { "ok" } else { "FAILED" }
            );
        }
        Self {
            layout,
            choice,
            core_off,
            core_len,
            failed_build,
            healthy,
        }
    }

    /// This core is on trial and has not confirmed yet.
    pub fn on_trial(&self) -> bool {
        self.choice
            .is_some_and(|c| c.slot.record.trial && !c.slot.marks.confirmed)
    }

    /// The loader skipped a newer core that never confirmed.
    pub fn rolled_back(&self) -> bool {
        self.choice.is_some_and(|c| c.rolled_back)
    }

    /// The running core's own bytes: never written while it runs.
    pub fn core_extent(&self) -> Extent {
        Extent {
            start: self.core_off,
            end: self.core_off + self.core_len,
        }
    }

    pub fn engine_extent(&self) -> Extent {
        self.layout.engine_extent(self.core_off, self.core_len)
    }

    /// The first thing a trial core does: say it ran. A core that dies before
    /// confirming is then a failed trial, and the loader rolls it back.
    pub fn mark_attempted(&self, flash: &mut SplitFlash) {
        if let Some(c) = self.choice
            && c.slot.record.trial
            && !c.slot.marks.attempted
        {
            flash.program_word(BOOT_RECORD_SECTORS[c.sector] + ATTEMPTED_MARK_OFFSET, 0);
        }
    }

    /// A trial core proves itself: from here the loader keeps booting it.
    pub fn confirm(&mut self, flash: &mut SplitFlash) {
        if !self.on_trial() {
            return;
        }
        if let Some(c) = self.choice.as_mut() {
            flash.program_word(BOOT_RECORD_SECTORS[c.sector] + CONFIRMED_MARK_OFFSET, 0);
            c.slot.marks.confirmed = true;
            super::say!("[OTA] core confirmed");
        }
    }
}

/// This boot follows a power-on or a brownout — the loader asks the same ROM
/// routine, so the two make the same choice (see `lp_bootctl::choose`).
fn cold_boot() -> bool {
    unsafe extern "C" {
        fn rtc_get_reset_reason(cpu: u32) -> u32;
    }
    // SAFETY: a ROM routine reading a status register.
    matches!(unsafe { rtc_get_reset_reason(0) }, 0x01 | 0x0F)
}

/// Length of the ESP image at `at`: header, then each segment's header and
/// bytes, then the checksum padding to 16 and the appended SHA-256.
fn image_len(flash: &mut SplitFlash, at: u32) -> Option<u32> {
    let mut header = [0u8; 24];
    if !flash.read(at, &mut header) || header[0] != 0xE9 {
        return None;
    }
    let mut off = at + 24;
    for _ in 0..header[1] {
        let mut seg = [0u8; 8];
        if !flash.read(off, &mut seg) {
            return None;
        }
        off += 8 + u32::from_le_bytes([seg[4], seg[5], seg[6], seg[7]]);
    }
    let len = (off - at + 1).div_ceil(16) * 16;
    let hash_appended = header[23] == 1;
    Some(len + if hash_appended { 32 } else { 0 })
}
