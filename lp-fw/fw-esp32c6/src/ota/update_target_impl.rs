//! `lpc_update`'s [`UpdateTarget`] over the split image: the fenced raw
//! flash ([`SplitFlash`]), layout 1's placement rules
//! ([`lp_bootctl::SplitLayout`]) and the split image's format hooks
//! (`lp-bootctl`'s boot record and engine header). The session never
//! re-types a format; this is where they meet.
//!
//! Every write goes through the fence the boot state set up: inside the
//! region and clear of the running core, or one of the record sectors. A
//! session asking for anything else is a bug, and is refused, logged and
//! reported to it as a [`FlashFault`].

use core::ops::Range;

use lp_bootctl::engine_header::{ENGINE_COMMIT_OFFSET, ENGINE_COMMITTED};
use lp_bootctl::{BOOT_RECORD_SECTORS, BootRecord, SplitLayout};
use lpc_update::board::{FlashFault, UpdateTarget};

use super::boot_state::BootState;
use super::split_flash::{BLOCK, SECTOR, SplitFlash};
use super::update_timing::{FlashTiming, now_us};

/// The split image's side of an update.
pub struct SplitUpdateTarget {
    flash: SplitFlash,
    layout: SplitLayout,
    core_off: u32,
    core_len: u32,
    /// The sector this boot's record came from, and its sequence number:
    /// a trial record goes in the other sector, one newer.
    record_sector: usize,
    record_seq: u32,
    /// Where this core's engine header is.
    engine_start: u32,
    /// What the session's flash calls cost (the `[OTA] timing` line).
    pub timing: FlashTiming,
}

impl SplitUpdateTarget {
    /// The target for this boot. When the boot state is not trusted the
    /// fence is never set, so nothing is ever written (and the session
    /// refuses every install `N`/`T` anyway).
    pub fn new(state: &BootState) -> Self {
        let mut flash = SplitFlash::take();
        let layout = match state.layout {
            Some(layout) => {
                if state.trusted() {
                    flash.protect(state.core_extent(), layout.region_end);
                }
                layout
            }
            None => SplitLayout {
                region_end: lp_bootctl::REGION_START,
                page: super::page_size(),
            },
        };
        let (record_sector, record_seq) = state
            .choice
            .map_or((0, 0), |c| (c.sector, c.slot.record.seq));
        Self {
            flash,
            layout,
            core_off: state.core_off,
            core_len: state.core_len,
            record_sector,
            record_seq,
            engine_start: state.engine_room().start,
            timing: FlashTiming::default(),
        }
    }

    /// The raw flash, for the core's own reads (the light's, the hashes').
    pub fn flash(&mut self) -> &mut SplitFlash {
        &mut self.flash
    }
}

fn ok(done: bool) -> Result<(), FlashFault> {
    if done { Ok(()) } else { Err(FlashFault) }
}

impl UpdateTarget for SplitUpdateTarget {
    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashFault> {
        let t0 = now_us();
        let done = self.flash.erase(addr);
        self.timing.erase_us += now_us() - t0;
        self.timing.erases += 1;
        ok(done)
    }

    fn block_size(&self) -> Option<u32> {
        Some(BLOCK)
    }

    fn erase_block(&mut self, addr: u32) -> Result<(), FlashFault> {
        let t0 = now_us();
        let done = self.flash.erase_block(addr);
        self.timing.block_us += now_us() - t0;
        self.timing.blocks += 1;
        ok(done)
    }

    fn program(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FlashFault> {
        let t0 = now_us();
        let done = self.flash.program(addr, bytes);
        self.timing.program_us += now_us() - t0;
        self.timing.programs += 1;
        ok(done)
    }

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashFault> {
        let t0 = now_us();
        let done = self.flash.read(addr, buf);
        self.timing.read_us += now_us() - t0;
        self.timing.reads += 1;
        ok(done)
    }

    fn core_dest(&self, core_len: u32) -> Option<u32> {
        self.layout
            .next_core_offset(self.core_off, self.core_len, core_len)
    }

    fn engine_room_for(&self, core_off: u32, core_len: u32) -> Range<u32> {
        let room = self.layout.engine_room(core_off, core_len);
        room.start..room.end
    }

    fn prepare_uncommitted_header(&self, sector0: &mut [u8]) {
        if let Some(word) = sector0.get_mut(ENGINE_COMMIT_OFFSET..ENGINE_COMMIT_OFFSET + 4) {
            word.fill(0xff);
        }
    }

    fn commit_engine_header(&mut self, dest: u32) -> Result<(), FlashFault> {
        ok(self
            .flash
            .program_word(dest + ENGINE_COMMIT_OFFSET as u32, ENGINE_COMMITTED))
    }

    fn write_trial_record(
        &mut self,
        dest: u32,
        len: u32,
        build_hash: u32,
    ) -> Result<(), FlashFault> {
        let record = BootRecord {
            seq: self.record_seq.wrapping_add(1),
            core_off: dest,
            core_len: len,
            build: build_hash,
            trial: true,
        };
        let at = BOOT_RECORD_SECTORS[1 - self.record_sector];
        ok(self.flash.erase(at) && self.flash.program(at, &record.encode()))
    }

    fn erase_engine_header(&mut self) -> Result<(), FlashFault> {
        debug_assert_eq!(self.engine_start % SECTOR, 0);
        ok(self.flash.erase(self.engine_start))
    }
}
