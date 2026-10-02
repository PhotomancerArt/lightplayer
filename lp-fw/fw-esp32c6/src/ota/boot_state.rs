//! What this boot is: which core is running, from which boot record, on
//! trial or not — and where that leaves room for the engine and the next core.
//!
//! Reads the same two records the loader read and makes the same choice
//! (`lp_bootctl::choose`), so the core and the loader agree by construction.

use lp_bootctl::{
    ATTEMPTED_MARK_OFFSET, BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootChoice, BootRecord,
    BootSlot, CONFIRMED_MARK_OFFSET, Extent, REGION_START, SplitLayout,
};

use super::engine_window::page_size;
use super::split_flash::{SectorBuf, SplitFlash};

pub struct BootState {
    pub layout: SplitLayout,
    pub choice: Option<BootChoice>,
    pub core_off: u32,
    pub core_len: u32,
    max_seq: u32,
}

impl BootState {
    pub fn read(flash: &mut SplitFlash) -> Self {
        let layout = SplitLayout::c6_4mb(page_size());
        let mut sectors = [None; 2];
        let mut max_seq = 0;
        for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
            let mut buf = [0u8; BOOT_RECORD_READ_LEN];
            if flash.read(at, &mut buf) {
                *slot = BootSlot::decode(&buf);
            }
            if let Some(s) = slot {
                max_seq = max_seq.max(s.record.seq);
            }
        }
        let choice = lp_bootctl::choose(sectors);
        let (core_off, core_len) = match choice {
            Some(c) => (c.slot.record.core_off, c.slot.record.core_len),
            // No record (a board flashed without one): the loader booted the
            // low end; its length is the image's own.
            None => (REGION_START, image_len(flash, REGION_START).unwrap_or(0)),
        };
        Self {
            layout,
            choice,
            core_off,
            core_len,
            max_seq,
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
            esp_println::println!("[OTA] core confirmed");
        }
    }

    /// Name a new core in the record sector this boot did NOT come from, on
    /// trial, with the next sequence number. Erase, then one write: a cut
    /// leaves either the old contents or a record that fails its CRC.
    pub fn write_trial_record(
        &self,
        flash: &mut SplitFlash,
        buf: &mut SectorBuf,
        core_off: u32,
        core_len: u32,
    ) {
        let sector = match self.choice {
            Some(c) => 1 - c.sector,
            None => 1,
        };
        let record = BootRecord {
            seq: self.max_seq + 1,
            core_off,
            core_len,
            trial: true,
        };
        flash.write_sector(buf, BOOT_RECORD_SECTORS[sector], &record.encode());
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
