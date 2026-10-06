//! What this boot is: which core is running, from which boot record, on
//! trial or not — and where that leaves room for the engine.
//!
//! Reads the same two records the loader read and makes the same choice
//! (`lp_bootctl::choose`, with the same reset classification), so the core
//! and the loader agree by construction. The state is **trusted** — and
//! anything is ever written — only when every read succeeded, the flashed
//! table gave a layout, and the records agree with the MMU about where this
//! core runs.

use alloc::vec;

use lp_bootctl::{
    ATTEMPTED_MARK_OFFSET, BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootChoice, BootSlot,
    COLD_TALLY_OFFSET, CONFIRMED_MARK_OFFSET, Extent, LOADER_OFFSET, REGION_START, ResetKind,
    STARTED_MARK_OFFSET, SplitLayout,
};

use super::engine_window::{page_size, running_core_offset};
use super::split_flash::SplitFlash;

/// Bounds an engine header's length when the flashed table could not be
/// read: the end of a 4 MB chip. Nothing is written in that state.
const FALLBACK_REGION_END: u32 = 0x40_0000;

pub struct BootState {
    /// The layout from the flashed table, when it gave one.
    pub layout: Option<SplitLayout>,
    pub choice: Option<BootChoice>,
    pub reset: ResetKind,
    pub core_off: u32,
    pub core_len: u32,
    /// The build that failed its trial here, when the loader rolled back.
    pub failed_build: Option<u32>,
    /// Why nothing is written this boot, or `None` when the state is trusted.
    pub untrusted: Option<&'static str>,
    /// This boot was counted as a cold retry of the trial.
    pub counted_cold_retry: bool,
    /// The core running is not the one the records choose: the loader fell
    /// back to the other record's core.
    pub fell_back: bool,
    /// The loader's version word (0: none).
    pub loader_version: u16,
}

impl BootState {
    /// A state that trusts nothing (what is left behind once the real one
    /// has been taken out of `CoreBoot`).
    pub fn placeholder() -> Self {
        Self {
            layout: None,
            choice: None,
            reset: ResetKind::Warm,
            core_off: 0,
            core_len: 0,
            failed_build: None,
            untrusted: Some("placeholder"),
            counted_cold_retry: false,
            fell_back: false,
            loader_version: 0,
        }
    }

    /// Read this boot's state. `factory` is `(offset, len)` from the flashed
    /// table, if it could be read.
    pub fn read(flash: &mut SplitFlash, factory: Option<(u32, u32)>, reset: ResetKind) -> Self {
        let page = page_size();
        let layout = factory.and_then(|(offset, len)| SplitLayout::from_factory(offset, len, page));
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
        let choice = lp_bootctl::choose(sectors, reset);
        let failed_build = choice
            .filter(|c| c.rolled_back)
            .and_then(|c| sectors[1 - c.sector])
            .map(|s| s.record.build);
        let running = running_core_offset();
        let (core_off, core_len) = match choice {
            Some(c) if c.slot.record.core_off == running => {
                (c.slot.record.core_off, c.slot.record.core_len)
            }
            // No record (a board flashed without one), or the loader fell
            // back to another record's core: the core is where the MMU says
            // it is, and its length is the image's own — so the engine's
            // room is worked out from the core that really runs.
            _ => (running, image_len(flash, running).unwrap_or(0)),
        };
        let records_disagree = choice.is_some_and(|c| c.slot.record.core_off != running);
        let untrusted = if factory.is_none() {
            Some("no `factory` in the flashed partition table")
        } else if layout.is_none() {
            Some("`factory` cannot hold this layout under this MMU page")
        } else if !reads_ok {
            Some("a boot-record read failed")
        } else if records_disagree || core_len == 0 {
            Some("the boot records disagree with the MMU about where this core runs")
        } else if !layout.is_some_and(|l| l.fits(core_off, core_len)) {
            Some("the core's extent does not fit the region")
        } else {
            None
        };
        let mut loader = vec![0u8; lp_bootctl::loader_identity::LOADER_ID_SCAN_LEN];
        let loader_version = if flash.read(LOADER_OFFSET, &mut loader) {
            lp_bootctl::find_loader_version(&loader)
        } else {
            0
        };
        Self {
            layout,
            choice,
            reset,
            core_off,
            core_len,
            failed_build,
            untrusted,
            counted_cold_retry: false,
            fell_back: records_disagree,
            loader_version,
        }
    }

    pub fn trusted(&self) -> bool {
        self.untrusted.is_none()
    }

    /// This core is on trial and has not confirmed yet.
    pub fn on_trial(&self) -> bool {
        self.choice
            .is_some_and(|c| c.slot.record.trial && !c.slot.marks.confirmed)
    }

    /// The loader skipped a newer core that failed its trial.
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

    /// The most the engine may occupy. Without a layout from the table, a
    /// low core's engine is bounded only by the chip: the header's own
    /// length decides how much is mapped.
    pub fn engine_room(&self) -> Extent {
        let layout = self.layout.unwrap_or(SplitLayout {
            region_end: FALLBACK_REGION_END,
            page: page_size(),
        });
        if self.layout.is_none() && self.core_off != REGION_START {
            return Extent { start: 0, end: 0 };
        }
        layout.engine_room(self.core_off, self.core_len)
    }

    /// The first write of a trial core's boot, before anything that could
    /// fail: count a cold retry when this boot is one, then mark attempted.
    /// A core that dies before confirming is then accountable.
    pub fn begin_trial(&mut self, flash: &mut SplitFlash) {
        let Some(c) = self.choice.as_mut() else {
            return;
        };
        if !c.slot.record.trial || c.rolled_back {
            return;
        }
        let sector = BOOT_RECORD_SECTORS[c.sector];
        if lp_bootctl::cold_retry_to_count(&c.slot, self.reset)
            && let Some(word) = c.slot.marks.next_cold_tally_word()
        {
            flash.program_word(sector + COLD_TALLY_OFFSET, word);
            c.slot.marks.cold_tally &= word;
            self.counted_cold_retry = true;
        }
        if !c.slot.marks.attempted {
            flash.program_word(sector + ATTEMPTED_MARK_OFFSET, 0);
            c.slot.marks.attempted = true;
        }
    }

    /// A trial core finished its bring-up (radios and links up): from here a
    /// power cycle is never counted against it.
    pub fn mark_started(&mut self, flash: &mut SplitFlash) {
        if !self.on_trial() {
            return;
        }
        if let Some(c) = self.choice.as_mut()
            && !c.slot.marks.started
        {
            flash.program_word(BOOT_RECORD_SECTORS[c.sector] + STARTED_MARK_OFFSET, 0);
            c.slot.marks.started = true;
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
            log::info!("[OTA] core confirmed");
        }
    }

    /// This boot's record carries its `confirmed` mark: a trial that proved
    /// itself, or a flashed record the engine guard has passed once
    /// ([`super::engine_guard`]).
    pub fn confirmed(&self) -> bool {
        self.choice.is_some_and(|c| c.slot.marks.confirmed)
    }

    /// The engine guard passed on a record that is not a trial (the first
    /// boot after a USB flash): mark it confirmed, so later boots skip the
    /// guard. The loader reads `confirmed` only on trials, so this changes
    /// no boot choice.
    pub fn confirm_checked(&mut self, flash: &mut SplitFlash) {
        if !self.trusted() {
            return;
        }
        if let Some(c) = self.choice.as_mut()
            && !c.slot.marks.confirmed
            && flash.program_word(BOOT_RECORD_SECTORS[c.sector] + CONFIRMED_MARK_OFFSET, 0)
        {
            c.slot.marks.confirmed = true;
        }
    }

    /// How this boot came to run this core, for the boot line.
    pub fn standing(&self) -> &'static str {
        match self.choice {
            _ if self.fell_back => "fallback",
            None => "no record",
            Some(c) if c.rolled_back => "rolled back",
            Some(c) if c.slot.record.trial && !c.slot.marks.confirmed => "trial",
            Some(_) => "proven",
        }
    }
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
