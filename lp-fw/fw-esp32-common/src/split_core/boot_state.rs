//! What this boot is: which core is running, from which boot record, on
//! trial or not — and where that leaves room for the engine.
//!
//! Reads the same two records the loader read and makes the same choice
//! (`lp_bootctl::choose`, with the same reset classification), so the core
//! and the loader agree by construction — and when the loader had to fall
//! back from an unconfirmed trial whose core would not load to the proven
//! core running here, the core reads it as the rollback it is
//! (`lp_bootctl::booted_by`). The state is **trusted** — and
//! anything is ever written — only when every read succeeded, the flashed
//! table gave a layout, and the records agree with the MMU about where this
//! core runs.

use alloc::vec;

use lp_bootctl::{
    ATTEMPTED_MARK_OFFSET, BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootChoice, BootSlot,
    COLD_TALLY_OFFSET, CONFIRMED_MARK_OFFSET, Extent, LOADER_OFFSET, ResetKind,
    STARTED_MARK_OFFSET, SplitLayout,
};

use super::split_chip::SplitChip;
use super::split_flash::SplitFlash;

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
    pub fn read<C: SplitChip>(
        flash: &mut SplitFlash<C>,
        factory: Option<(u32, u32)>,
        reset: ResetKind,
    ) -> Self {
        let layout = factory.and_then(|(offset, len)| C::layout(offset, len));
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
        let running = C::running_core_offset();
        // The loader's choice — or, when it fell back from an unconfirmed
        // trial whose core would not load to the proven core running here,
        // that rollback (`booted_by`; defect 2026-10-17, a power cycle after
        // a heal).
        let choice = lp_bootctl::booted_by(sectors, reset, running);
        let failed_build = choice
            .filter(|c| c.rolled_back)
            .and_then(|c| sectors[1 - c.sector])
            .map(|s| s.record.build);
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
        let untrusted = if let Some(why) = C::updates_off() {
            Some(why)
        } else if factory.is_none() {
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
    /// low core's engine is bounded only by the chip
    /// ([`SplitChip::FALLBACK_REGION_END`]): the header's own length decides
    /// how much is mapped.
    pub fn engine_room<C: SplitChip>(&self) -> Extent {
        let layout = self.layout.unwrap_or(SplitLayout {
            region_start: C::REGION_START,
            region_end: C::FALLBACK_REGION_END,
            page: C::page_size(),
        });
        if self.layout.is_none() && self.core_off != C::REGION_START {
            return Extent { start: 0, end: 0 };
        }
        layout.engine_room(self.core_off, self.core_len)
    }

    /// The first write of a trial core's boot, before anything that could
    /// fail: count a cold retry when this boot is one, then mark attempted.
    /// A core that dies before confirming is then accountable.
    pub fn begin_trial<C: SplitChip>(&mut self, flash: &mut SplitFlash<C>) {
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
    pub fn mark_started<C: SplitChip>(&mut self, flash: &mut SplitFlash<C>) {
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
    pub fn confirm<C: SplitChip>(&mut self, flash: &mut SplitFlash<C>) {
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
    pub fn confirm_checked<C: SplitChip>(&mut self, flash: &mut SplitFlash<C>) {
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
fn image_len<C: SplitChip>(flash: &mut SplitFlash<C>, at: u32) -> Option<u32> {
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

#[cfg(test)]
mod tests {
    use lp_bootctl::{BootRecord, BootSlot, REGION_START};

    use super::super::test_chip::TestChip;
    use super::*;

    /// The C6's `factory`: `0x10000`, ending at `0x380000`.
    const FACTORY: Option<(u32, u32)> = Some((0x1_0000, 0x37_0000));

    /// An ESP image at `at`: one segment of `seg` bytes, a hash appended.
    fn image(at: u32, seg: u32) -> u32 {
        let mut header = [0u8; 24];
        header[0] = 0xE9;
        header[1] = 1;
        header[23] = 1;
        TestChip::flash(at, &header);
        let mut seg_header = [0u8; 8];
        seg_header[4..].copy_from_slice(&seg.to_le_bytes());
        TestChip::flash(at + 24, &seg_header);
        (24 + 8 + seg + 1).div_ceil(16) * 16 + 32
    }

    fn record(sector: usize, core_off: u32, core_len: u32, trial: bool) {
        let record = BootRecord {
            seq: 1,
            core_off,
            core_len,
            build: 0x1234_5678,
            trial,
        };
        TestChip::flash(BOOT_RECORD_SECTORS[sector], &record.encode());
    }

    fn read() -> BootState {
        let mut flash = SplitFlash::<TestChip>::take();
        BootState::read(&mut flash, FACTORY, TestChip::reset_kind())
    }

    #[test]
    fn a_core_with_no_record_is_where_the_mmu_says_and_as_long_as_its_image() {
        TestChip::reset(REGION_START, ResetKind::Warm);
        let len = image(REGION_START, 0x1_0000);
        let state = read();
        assert_eq!(state.untrusted, None);
        assert_eq!((state.core_off, state.core_len), (REGION_START, len));
        assert_eq!(state.standing(), "no record");
        assert_eq!(state.engine_room::<TestChip>().start, 0x3_0000);
    }

    #[test]
    fn records_that_disagree_with_the_mmu_are_never_trusted() {
        TestChip::reset(0x20_0000, ResetKind::Warm);
        image(0x20_0000, 0x1_0000);
        record(0, REGION_START, 0x1_0000, false);
        let state = read();
        assert_eq!(
            state.untrusted,
            Some("the boot records disagree with the MMU about where this core runs")
        );
        assert!(state.fell_back);
    }

    #[test]
    fn no_factory_in_the_table_writes_nothing() {
        TestChip::reset(REGION_START, ResetKind::Warm);
        record(0, REGION_START, 0x1_0000, true);
        let state = super::super::begin::<TestChip>(None);
        assert!(!state.trusted());
        let slot = BootSlot::decode(&TestChip::bytes(
            BOOT_RECORD_SECTORS[0],
            BOOT_RECORD_READ_LEN,
        ));
        assert!(!slot.unwrap().marks.attempted);
    }

    #[test]
    fn a_part_that_takes_no_update_writes_nothing_and_says_why() {
        TestChip::reset(REGION_START, ResetKind::Warm);
        TestChip::updates_off("octal flash");
        record(0, REGION_START, 0x1_0000, true);
        let state = super::super::begin::<TestChip>(FACTORY);
        assert_eq!(state.untrusted, Some("octal flash"));
        assert!(
            state.layout.is_some(),
            "the engine's room still follows the layout"
        );
        let slot = BootSlot::decode(&TestChip::bytes(
            BOOT_RECORD_SECTORS[0],
            BOOT_RECORD_READ_LEN,
        ));
        assert!(
            !slot.unwrap().marks.attempted,
            "no mark on a part that forbids them"
        );
    }

    #[test]
    fn a_trial_core_marks_itself_attempted_then_started_and_confirmed() {
        TestChip::reset(REGION_START, ResetKind::Warm);
        record(0, REGION_START, 0x1_0000, true);
        let mut state = super::super::begin::<TestChip>(FACTORY);
        assert!(state.trusted() && state.on_trial());
        let marks = || {
            BootSlot::decode(&TestChip::bytes(
                BOOT_RECORD_SECTORS[0],
                BOOT_RECORD_READ_LEN,
            ))
            .unwrap()
            .marks
        };
        assert!(marks().attempted && !marks().started);
        super::super::mark_started::<TestChip>(&mut state);
        assert!(marks().started && !marks().confirmed);
        let mut flash = SplitFlash::<TestChip>::take();
        flash.protect(state.core_extent(), 0x38_0000);
        state.confirm(&mut flash);
        assert!(marks().confirmed);
        assert_eq!(state.standing(), "proven");
    }
}
