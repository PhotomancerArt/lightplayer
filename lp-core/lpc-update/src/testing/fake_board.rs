//! A board, as simply as the tests need one: a [`NorFlash`], a catalog of
//! the builds it may hold, and a boot model.
//!
//! **A model, not the split image's formats.** It keeps layout 1's offsets
//! (factory at 0: progress record `0x5000`, boot records `0x6000`/`0x7000`,
//! the region from `0x8000`) and the split image's placement rule (the core
//! alternates between the region's two ends, the engine fills what the core
//! leaves), with a 4 KiB page. Its boot record and engine header are its
//! own:
//!
//! - **Boot record** (two sectors, the newer valid one wins):
//!   `"FBR1" seq core_off core_len build trial:u8 pad[3] crc(0..24)`, then
//!   mark bytes outside the CRC, programmed `0x00`: **failed** (the trial
//!   crashed warm: [`FakeBoard::fail_trial`]), **confirmed** (a link came up
//!   on it), and a **cold tally** of three (one per boot of an unconfirmed
//!   trial, the split image's cold cap). A trial that failed, or used up its
//!   three cold boots unconfirmed, rolls back to the other record, and the
//!   board refuses that build.
//! - **Engine header**: [`ModelBuild`]'s first 12 bytes. The engine is
//!   valid iff its magic and commit word are in place and its bytes hash to
//!   the core's digest slot.
//!
//! [`FakeBoard::check_invariant`] is what a power-cut test asserts after
//! every cut: a bootable core exists, and no committed engine header sits
//! over bytes that do not hash.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use sha2::{Digest, Sha256};

use crate::board::{BoardFacts, EngineStatus, FlashFault, SessionMode, UpdateTarget};
use crate::code_table::{CHIP_ESP32C6, CHUNK, LAYOUT_1, LOADER_1};

use super::model_build::{MODEL_COMMIT_AT, MODEL_COMMIT_WORD, MODEL_ENGINE_MAGIC, ModelBuild};
use super::nor_flash::NorFlash;

/// Layout 1's progress-record sector, factory at 0.
pub const MODEL_PROGRESS: u32 = 0x5000;
/// The two boot-record sectors.
pub const MODEL_RECORDS: [u32; 2] = [0x6000, 0x7000];
/// The region's start.
pub const MODEL_REGION_START: u32 = 0x8000;
/// The model's page.
pub const MODEL_PAGE: u32 = CHUNK;
/// The model's block erase: four sectors, so a model piece of a few chunks
/// crosses whole blocks (the C6's part erases 64 KiB).
pub const MODEL_BLOCK: u32 = 4 * CHUNK;

const RECORD_MAGIC: [u8; 4] = *b"FBR1";
const RECORD_CRC_AT: usize = 24;
const FAILED_AT: usize = 28;
const CONFIRMED_AT: usize = 29;
const TALLY_AT: usize = 30;
const COLD_CAP: usize = 3;

/// The target name every model board reports.
pub const MODEL_TARGET: &str = "esp32c6-4mb";
/// The `wireProto` every model board reports.
pub const MODEL_WIRE_PROTO: u32 = 36;

/// A decoded model boot record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelRecord {
    sector: usize,
    seq: u32,
    core_off: u32,
    core_len: u32,
    build: u32,
    trial: bool,
    failed: bool,
    confirmed: bool,
    cold_tally: usize,
}

/// The core a board booted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunningCore {
    pub core_off: u32,
    pub core_len: u32,
    /// Index into the catalog.
    pub build: usize,
}

/// What a boot found wrong: each is an E-strand finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootFault {
    /// No record names a core whose bytes are a known build.
    NoBootableCore(String),
    /// A committed engine header sits over bytes that do not hash.
    TornEngine(String),
    /// The power was cut during the boot itself (its trial tally write):
    /// not a finding, the board is simply off.
    PowerCut,
}

/// The model board.
#[derive(Clone, Debug)]
pub struct FakeBoard {
    pub flash: NorFlash,
    pub region_end: u32,
    pub catalog: Vec<ModelBuild>,
    /// Set by [`FakeBoard::boot`].
    pub running: Option<RunningCore>,
    /// `false` makes every boot report an untrusted boot state (`N`/`T`).
    pub trusted_boot: bool,
    /// The block erase the board offers ([`MODEL_BLOCK`] by default);
    /// `None` erases sector by sector.
    pub block: Option<u32>,
}

impl FakeBoard {
    /// A board factory-flashed with `catalog[build]`: one confirmed boot
    /// record, the core at the region's low end, its engine committed.
    #[must_use]
    pub fn flashed_with(catalog: Vec<ModelBuild>, build: usize, region_len: u32) -> Self {
        let region_end = MODEL_REGION_START + region_len;
        let mut board = Self {
            flash: NorFlash::new(region_end as usize),
            region_end,
            catalog,
            running: None,
            trusted_boot: true,
            block: Some(MODEL_BLOCK),
        };
        let b = board.catalog[build].clone();
        let core_off = MODEL_REGION_START;
        let core_len = b.core.len() as u32;
        board.flash.flash_image(core_off, &b.core);
        let room = board.engine_room_at(core_off, core_len);
        board.flash.flash_image(room.start, &b.engine);
        let rec = encode_record(1, core_off, core_len, b.build_hash(), false);
        board.flash.flash_image(MODEL_RECORDS[0], &rec);
        board
    }

    /// Boot: choose a core, find its engine, and say what the session
    /// needs to know. Counts a cold boot of an unconfirmed trial core, and
    /// rolls back a trial that failed or used up its cold boots.
    pub fn boot(&mut self) -> Result<BoardFacts, BootFault> {
        self.running = None;
        let records = self.records();
        let newest = records.iter().max_by_key(|r| r.seq).copied();
        let Some(mut chosen) = newest else {
            return Err(BootFault::NoBootableCore("no boot record".into()));
        };
        let mut refused_build = None;
        let mut on_trial = false;
        if chosen.trial && !chosen.confirmed {
            if chosen.failed || chosen.cold_tally >= COLD_CAP {
                refused_build = Some(chosen.build);
                chosen = records
                    .iter()
                    .copied()
                    .find(|r| r.sector != chosen.sector)
                    .ok_or_else(|| {
                        BootFault::NoBootableCore("rollback with no other record".into())
                    })?;
            } else {
                let at = MODEL_RECORDS[chosen.sector] + (TALLY_AT + chosen.cold_tally) as u32;
                self.flash
                    .program(at, &[0])
                    .map_err(|_| BootFault::PowerCut)?;
                on_trial = true;
            }
        }
        let build = self
            .build_at(chosen.core_off, chosen.core_len)
            .ok_or_else(|| {
                BootFault::NoBootableCore(format!(
                    "the record names {:#x}+{:#x}, which is no known core",
                    chosen.core_off, chosen.core_len
                ))
            })?;
        self.running = Some(RunningCore {
            core_off: chosen.core_off,
            core_len: chosen.core_len,
            build,
        });
        let b = &self.catalog[build];
        let engine = match self.engine_state(chosen.core_off, chosen.core_len, build)? {
            Some(_) if b.crashes => EngineStatus::Crashing,
            Some(_) => EngineStatus::Valid,
            None => EngineStatus::Missing,
        };
        let mode = if engine == EngineStatus::Valid && !on_trial {
            SessionMode::EngineRunning
        } else {
            SessionMode::CoreOnly
        };
        Ok(BoardFacts {
            mode,
            chip: CHIP_ESP32C6,
            chip_word: "esp32c6".into(),
            layout: LAYOUT_1,
            loader: LOADER_1,
            target: MODEL_TARGET.into(),
            version: b.version.clone(),
            wire_proto: MODEL_WIRE_PROTO,
            build_id: b.build_id_field(),
            core_sha256: b.core_sha256(),
            digest_slot: b.engine_sha256(),
            core_off: chosen.core_off,
            core_len: chosen.core_len,
            engine_len: (engine != EngineStatus::Missing).then_some(b.engine.len() as u32),
            region_len: self.region_end - MODEL_REGION_START,
            trusted_boot: self.trusted_boot,
            on_trial,
            refused_build,
            engine,
            progress_record_addr: MODEL_PROGRESS,
        })
    }

    /// The power-cut invariant, without changing the flash: a bootable core
    /// exists, and no committed engine header sits over bytes that do not
    /// hash (for the core that would boot, and for either end's core).
    pub fn check_invariant(&self) -> Result<(), BootFault> {
        let mut probe = self.clone();
        probe.flash.power_on();
        probe.boot().map(|_| ())
    }

    /// The build of the core that is running, if booted.
    #[must_use]
    pub fn running_build(&self) -> Option<&ModelBuild> {
        self.running.map(|r| &self.catalog[r.build])
    }

    /// Program the confirmed mark of the newest record (the firmware's answer
    /// to [`Effect::TrialProof`](crate::board::Effect::TrialProof)).
    pub fn confirm_trial(&mut self) -> Result<(), FlashFault> {
        let Some(r) = self.records().into_iter().max_by_key(|r| r.seq) else {
            return Ok(());
        };
        if r.trial && !r.confirmed {
            self.flash
                .program(MODEL_RECORDS[r.sector] + CONFIRMED_AT as u32, &[0])?;
        }
        Ok(())
    }

    /// The trial core crashed warm (the split image's `fixture-trial-dies`):
    /// the next boot rolls it back.
    pub fn fail_trial(&mut self) {
        if let Some(r) = self.records().into_iter().max_by_key(|r| r.seq)
            && r.trial
            && !r.confirmed
        {
            self.flash
                .flash_image(MODEL_RECORDS[r.sector] + FAILED_AT as u32, &[0]);
        }
    }

    /// Whether the running core's engine is valid right now.
    #[must_use]
    pub fn engine_valid(&self) -> bool {
        self.running.is_some_and(|r| {
            matches!(
                self.engine_state(r.core_off, r.core_len, r.build),
                Ok(Some(_))
            )
        })
    }

    fn records(&self) -> Vec<ModelRecord> {
        let mut out = Vec::new();
        for (sector, &addr) in MODEL_RECORDS.iter().enumerate() {
            let at = addr as usize;
            let Some(b) = self.flash.bytes().get(at..at + 40) else {
                continue;
            };
            if b[0..4] != RECORD_MAGIC {
                continue;
            }
            let word = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
            if word(RECORD_CRC_AT) != lp_crc32::crc32(&b[..RECORD_CRC_AT]) {
                continue;
            }
            out.push(ModelRecord {
                sector,
                seq: word(4),
                core_off: word(8),
                core_len: word(12),
                build: word(16),
                trial: b[20] == 1,
                failed: b[FAILED_AT] != 0xFF,
                confirmed: b[CONFIRMED_AT] != 0xFF,
                cold_tally: b[TALLY_AT..TALLY_AT + COLD_CAP]
                    .iter()
                    .filter(|&&m| m != 0xFF)
                    .count(),
            });
        }
        out
    }

    /// The catalog build whose core is at `[off, off + len)`, by hash.
    fn build_at(&self, off: u32, len: u32) -> Option<usize> {
        let bytes = self.flash.bytes().get(off as usize..(off + len) as usize)?;
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        self.catalog.iter().position(|b| b.core_sha256() == hash)
    }

    /// `Ok(Some(len))`: a valid engine; `Ok(None)`: none;
    /// `Err(TornEngine)`: a committed header over bytes that do not hash.
    fn engine_state(
        &self,
        core_off: u32,
        core_len: u32,
        build: usize,
    ) -> Result<Option<u32>, BootFault> {
        let room = self.engine_room_at(core_off, core_len);
        let f = self.flash.bytes();
        let at = room.start as usize;
        let header = &f[at..at + 12];
        if header[0..4] != MODEL_ENGINE_MAGIC
            || header[MODEL_COMMIT_AT..MODEL_COMMIT_AT + 4] != MODEL_COMMIT_WORD
        {
            return Ok(None);
        }
        let len = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let fits = room
            .start
            .checked_add(len)
            .is_some_and(|end| end <= room.end);
        let hash: Option<[u8; 32]> = fits.then(|| Sha256::digest(&f[at..at + len as usize]).into());
        if hash == Some(self.catalog[build].engine_sha256()) {
            Ok(Some(len))
        } else {
            Err(BootFault::TornEngine(format!(
                "a committed engine header at {:#x} over bytes that do not hash",
                room.start
            )))
        }
    }

    fn up(x: u32) -> u32 {
        x.div_ceil(MODEL_PAGE) * MODEL_PAGE
    }

    fn engine_room_at(&self, core_off: u32, core_len: u32) -> Range<u32> {
        if core_off == MODEL_REGION_START {
            Self::up(core_off + core_len)..self.region_end
        } else {
            MODEL_REGION_START..core_off
        }
    }

    fn running_extent(&self) -> Option<Range<u32>> {
        self.running
            .map(|r| r.core_off..Self::up(r.core_off + r.core_len))
    }

    /// The fence: writes go to the progress sector or the region, never over
    /// the running core.
    fn fenced(&self, addr: u32, len: u32) -> Result<(), FlashFault> {
        let end = addr.checked_add(len.max(1)).ok_or(FlashFault)?;
        let progress = MODEL_PROGRESS..MODEL_PROGRESS + CHUNK;
        let in_progress = addr >= progress.start && end <= progress.end;
        let in_region = addr >= MODEL_REGION_START && end <= self.region_end;
        let over_core = self
            .running_extent()
            .is_some_and(|c| addr < c.end && end > c.start);
        if (in_progress || in_region) && !over_core {
            Ok(())
        } else {
            Err(FlashFault)
        }
    }
}

impl UpdateTarget for FakeBoard {
    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashFault> {
        self.fenced(addr / CHUNK * CHUNK, CHUNK)?;
        self.flash.erase(addr)
    }

    fn block_size(&self) -> Option<u32> {
        self.block
    }

    fn erase_block(&mut self, addr: u32) -> Result<(), FlashFault> {
        let block = self.block.ok_or(FlashFault)?;
        if addr % block != 0 {
            return Err(FlashFault);
        }
        self.fenced(addr, block)?;
        self.flash.erase_block(addr, block)
    }

    fn program(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FlashFault> {
        self.fenced(addr, bytes.len() as u32)?;
        self.flash.program(addr, bytes)
    }

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashFault> {
        self.flash.read(addr, buf)
    }

    fn core_dest(&self, core_len: u32) -> Option<u32> {
        let r = self.running?;
        if r.core_off == MODEL_REGION_START {
            let at = self.region_end.checked_sub(core_len)? / MODEL_PAGE * MODEL_PAGE;
            (at >= Self::up(r.core_off + r.core_len)).then_some(at)
        } else {
            (MODEL_REGION_START + core_len <= r.core_off).then_some(MODEL_REGION_START)
        }
    }

    fn engine_room_for(&self, core_off: u32, core_len: u32) -> Range<u32> {
        self.engine_room_at(core_off, core_len)
    }

    fn prepare_uncommitted_header(&self, sector0: &mut [u8]) {
        if let Some(word) = sector0.get_mut(MODEL_COMMIT_AT..MODEL_COMMIT_AT + 4) {
            word.fill(0xFF);
        }
    }

    fn commit_engine_header(&mut self, dest: u32) -> Result<(), FlashFault> {
        self.program(dest + MODEL_COMMIT_AT as u32, &MODEL_COMMIT_WORD)
    }

    fn write_trial_record(
        &mut self,
        dest: u32,
        len: u32,
        build_hash: u32,
    ) -> Result<(), FlashFault> {
        // Never over the record the running core booted from: after a
        // rollback the newest record is the failed trial's, not ours.
        let records = self.records();
        let seq = records.iter().map(|r| r.seq).max().unwrap_or(0) + 1;
        let running = self.running.ok_or(FlashFault)?;
        let ours = records
            .iter()
            .filter(|r| r.core_off == running.core_off && r.core_len == running.core_len)
            .max_by_key(|r| r.seq)
            .map_or(1, |r| r.sector);
        let sector = 1 - ours;
        let addr = MODEL_RECORDS[sector];
        self.flash.erase(addr)?;
        self.flash
            .program(addr, &encode_record(seq, dest, len, build_hash, true))
    }

    fn erase_engine_header(&mut self) -> Result<(), FlashFault> {
        let r = self.running.ok_or(FlashFault)?;
        let room = self.engine_room_at(r.core_off, r.core_len);
        self.erase_sector(room.start)
    }
}

fn encode_record(seq: u32, core_off: u32, core_len: u32, build: u32, trial: bool) -> [u8; 32] {
    let mut b = [0xFFu8; 32];
    b[0..4].copy_from_slice(&RECORD_MAGIC);
    b[4..8].copy_from_slice(&seq.to_le_bytes());
    b[8..12].copy_from_slice(&core_off.to_le_bytes());
    b[12..16].copy_from_slice(&core_len.to_le_bytes());
    b[16..20].copy_from_slice(&build.to_le_bytes());
    b[20] = u8::from(trial);
    b[21..24].fill(0);
    let crc = lp_crc32::crc32(&b[..RECORD_CRC_AT]);
    b[24..28].copy_from_slice(&crc.to_le_bytes());
    b
}
