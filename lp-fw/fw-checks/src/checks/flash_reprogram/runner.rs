//! The payload's flow over an injected flash: re-check, scan, torn words,
//! control, re-program, prepare, and the work loop.
//!
//! The firmware supplies a [`ReprogramFlash`] (esp-storage on the C6) and a
//! microsecond clock; everything else is here, so the whole boot runs on the
//! host against a fake part with cuts and weak bits in it (the tests below).

use core::fmt;

use super::layout::{ProgramOrder, ReprogramLayout};
use super::pattern::{fill, intended_word};
use super::records::{
    BootRecord, EraseControlRecord, EraseScanRecord, EraseSectorRecord, PrepareRecord, ReadsRecord,
    ReprogramRecord, ScanRecord, TimingRecord, TornWord, WindowRecord, WordReads, WriteRecord,
};
use super::state::{self, ENTRY_SIZE, State};
use super::{
    CONTROL_CHUNK_READS, CONTROL_CHUNKS, CONTROL_GAP_US, ERASE_SECTORS, LoopMode, MAX_GROUPS,
    MAX_KEEP, MAX_SECTORS, MAX_TORN, MAX_WINDOW, REGION_FIRST, SECTOR_CHUNK_READS, SECTOR_SIZE,
    TIMING_WRITES, WINDOW_READS, WORD_SIZE, WRITE_SIZE, mode_of,
};

/// The flash the payload drives. Addresses are absolute.
pub trait ReprogramFlash {
    type Error: fmt::Debug;
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error>;
    /// Erase the sectors in `from..to` (both sector-aligned).
    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error>;
    /// Program `data` (at most one write, 4-aligned) at `addr`.
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error>;
}

/// Where a record goes: the firmware prints it behind `[fw-check-json] `.
pub type Emit<'a> = &'a mut dyn FnMut(&dyn fmt::Display);

/// A microsecond clock.
pub type Clock<'a> = &'a mut dyn FnMut() -> u64;

/// Working memory for one boot, about 20.5 KiB. The firmware puts it on the
/// heap; nothing here allocates.
pub struct Buffers {
    pub and: [u8; MAX_WINDOW],
    pub or: [u8; MAX_WINDOW],
    /// A second pair, for an erase-mode stage read against the window.
    pub and2: [u8; SECTOR_SIZE],
    pub or2: [u8; SECTOR_SIZE],
    pub read: [u8; SECTOR_SIZE],
    pub write: [u8; WRITE_SIZE],
}

impl Buffers {
    pub const fn new() -> Self {
        Self {
            and: [0; MAX_WINDOW],
            or: [0; MAX_WINDOW],
            and2: [0; SECTOR_SIZE],
            or2: [0; SECTOR_SIZE],
            read: [0; SECTOR_SIZE],
            write: [0; WRITE_SIZE],
        }
    }
}

impl Default for Buffers {
    fn default() -> Self {
        Self::new()
    }
}

/// Who is booting.
#[derive(Clone, Copy, Debug)]
pub struct BootFacts<'a> {
    /// The reset reason, as the firmware names it (`poweron` after a cut).
    pub reset: &'a str,
    pub mac: [u8; 6],
    pub flash_id: u32,
}

/// What the work loop programs after this boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Armed {
    pub order: ProgramOrder,
    pub generation: u32,
}

/// One boot, every record emitted as it is made. Writes nothing before the
/// re-check and the scan have read what the cut left.
pub fn boot<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    bufs: &mut Buffers,
    facts: BootFacts<'_>,
    now_us: Clock<'_>,
    emit: Emit<'_>,
) -> Result<Armed, F::Error> {
    flash.read(layout.state_addr(), &mut bufs.read)?;
    let (latest, free) = state::scan(&bufs.read);
    let Some(prev) = latest else {
        return fresh(flash, layout, bufs, facts, now_us, emit);
    };
    emit(&BootRecord {
        reset: facts.reset,
        mac: facts.mac,
        flash_id: facts.flash_id,
        base: layout.base,
        sectors: layout.sectors,
        fresh: false,
        seq: prev.seq,
        generation: prev.generation,
        keep: prev.keep,
        pending: prev.pending_list().count(),
        loop_mode: mode_of(prev.generation).name(),
    });

    // 1. The words (or, after an erase loop's boot, the sectors) the last
    //    boot re-programmed, after this boot's cut.
    let sectors_pending = mode_of(prev.pending_generation) == LoopMode::Erase
        && prev.pending_list().next().is_none()
        && prev.group_list().next().is_none();
    if sectors_pending {
        for s in prev.keep_list() {
            let addr = layout.sector_addr(s);
            sector_reads(flash, addr, CONTROL_CHUNKS, bufs, now_us)?;
            let r = classify_sector(&bufs.and2, &bufs.or2, None);
            emit(&EraseSectorRecord {
                stage: "recheck",
                addr,
                index: u32::MAX,
                reads: (SECTOR_CHUNK_READS * CONTROL_CHUNKS) as u32,
                ..r
            });
        }
    }
    for (write, span) in prev.group_list() {
        let r = check_write(flash, write, span, prev.pending_generation, bufs)?;
        emit(&WriteRecord {
            stage: "recheck",
            ..r
        });
    }
    for addr in prev.pending_list() {
        let r = read_word(
            flash,
            addr,
            intended_word(addr, prev.pending_generation),
            now_us,
        )?;
        emit(&ReadsRecord {
            stage: "recheck",
            reads: &r,
            gap_us: CONTROL_GAP_US,
        });
    }

    let keep: [u32; MAX_KEEP] = {
        let mut k = [u32::MAX; MAX_KEEP];
        for (i, s) in prev.keep_list().enumerate() {
            k[i] = s;
        }
        k
    };
    let order = ProgramOrder::new(*layout, &keep);
    let out = match mode_of(prev.generation) {
        LoopMode::Program => {
            program_experiment(flash, layout, &order, prev.generation, bufs, now_us, emit)?
        }
        LoopMode::Erase => {
            erase_experiment(flash, layout, &order, prev.generation, bufs, now_us, emit)?
        }
    };
    let Outcome {
        dirty: scan_dirty,
        keep_next,
        torn,
        n,
        groups,
    } = out;

    // 6. Keep the torn words' sectors, erase every other dirty one.
    let mut dirty = scan_dirty;
    for s in prev.keep_list() {
        dirty.set(s);
    }
    for s in keep_next.iter().flatten() {
        dirty.clear(*s);
    }
    let next = State {
        seq: prev.seq + 1,
        generation: prev.generation + 1,
        keep: keep_next,
        pending_generation: prev.generation,
        pending: {
            let mut p = [None; MAX_TORN];
            for (i, t) in torn[..n].iter().enumerate() {
                p[i] = Some(t.addr);
            }
            p
        },
        groups,
    };
    let (erased, erase_us, blank) = erase_dirty(flash, layout, &dirty, bufs, now_us)?;
    let keep: [u32; MAX_KEEP] = keep_next.map(|k| k.unwrap_or(u32::MAX));
    let armed = Armed {
        order: ProgramOrder::new(*layout, &keep),
        generation: next.generation,
    };
    let programmed_sectors = arm_erase_loop(flash, &armed, bufs)?;
    emit(&PrepareRecord {
        erased_sectors: erased,
        erase_us,
        blank,
        keep: keep_next,
        pending: n,
        next_generation: next.generation,
        programmed_sectors,
    });
    write_state(flash, layout, free, &next)?;
    emit(&timing(flash, layout, bufs, now_us)?);
    Ok(armed)
}

/// What one boot's experiment leaves for the next: what to erase, what to
/// keep, and the words it re-programmed.
struct Outcome {
    dirty: SectorSet,
    keep_next: [Option<u32>; MAX_KEEP],
    torn: [TornWord; MAX_TORN],
    n: usize,
    groups: [Option<(u32, u32)>; MAX_GROUPS],
}

/// After a program loop: the frontier, its torn words, the control, the
/// re-program and the reads after it.
fn program_experiment<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    order: &ProgramOrder,
    generation: u32,
    bufs: &mut Buffers,
    now_us: Clock<'_>,
    emit: Emit<'_>,
) -> Result<Outcome, F::Error> {
    // 2. The region, once, in the loop's order.
    let t0 = now_us();
    let scan = scan_region(flash, order, generation, bufs)?;
    let scan_us = now_us() - t0;
    emit(&ScanRecord {
        generation,
        order_bytes: order.len(),
        frontier: scan.frontier,
        frontier_addr: scan.frontier.map(|f| order.addr(f)),
        last_dirty: scan.last_dirty,
        span_words: scan.span.0 + scan.span.1 + scan.span.2,
        span_eq: scan.span.0,
        span_ff: scan.span.1,
        span_other: scan.span.2,
        scan_us,
    });

    // 3. The torn words around the frontier.
    let mut torn = [TornWord::default(); MAX_TORN];
    let mut groups: [Option<(u32, u32)>; MAX_GROUPS] = [None; MAX_GROUPS];
    let mut keep_next: [Option<u32>; MAX_KEEP] = [None; MAX_KEEP];
    let mut n = 0;
    if let Some(frontier) = scan.frontier {
        let w = window(flash, order, generation, frontier, scan.last_dirty, bufs)?;
        // Follow torn words while their writes fit in the groups and their
        // sectors in the kept set.
        for t in w.torn.iter().take(w.found.min(MAX_TORN)) {
            let write = align_down(t.addr, WRITE_SIZE as u32);
            if !groups.iter().flatten().any(|&(a, _)| a == write) {
                let sector = layout.sector_of(write);
                let Some(slot) = groups.iter().position(Option::is_none) else {
                    continue;
                };
                if !keep_next.contains(&Some(sector)) {
                    let Some(k) = keep_next.iter().position(Option::is_none) else {
                        continue;
                    };
                    keep_next[k] = Some(sector);
                }
                groups[slot] = Some((write, span_of(w.start, t.at, &bufs.and)));
            }
            torn[n] = *t;
            n += 1;
        }
        emit(&WindowRecord {
            start: w.start,
            end: w.end,
            reads: WINDOW_READS,
            capped: w.capped,
            weak_words: w.weak_words,
            weak_bits: w.weak_bits,
            torn_words: w.found as u32,
            followed: n,
        });
        for t in &torn[..n] {
            emit(t);
        }
    }

    // 4. The control: each torn word read many times, left alone.
    for t in &torn[..n] {
        let r = read_word(flash, t.addr, t.intended, now_us)?;
        emit(&ReadsRecord {
            stage: "before",
            reads: &r,
            gap_us: CONTROL_GAP_US,
        });
    }

    // 5. Re-program each torn write's span with what it was meant to hold,
    //    then read it back.
    for &(write, span) in groups.iter().flatten() {
        fill(write, generation, &mut bufs.write[..span as usize]);
        let t0 = now_us();
        flash.program(write, &bufs.write[..span as usize])?;
        let us = now_us() - t0;
        emit(&ReprogramRecord { write, span, us });
    }
    for &(write, span) in groups.iter().flatten() {
        let r = check_write(flash, write, span, generation, bufs)?;
        emit(&WriteRecord {
            stage: "after",
            ..r
        });
    }
    for t in &torn[..n] {
        let r = read_word(flash, t.addr, t.intended, now_us)?;
        emit(&ReadsRecord {
            stage: "after",
            reads: &r,
            gap_us: CONTROL_GAP_US,
        });
    }

    Ok(Outcome {
        dirty: scan.dirty,
        keep_next,
        torn,
        n,
        groups,
    })
}

/// After an erase loop: the first sector the loop had not finished and the
/// last it had, each read 64 times; a sector that is neither stably erased
/// nor stably its old pattern is torn, and is read left alone, programmed to
/// `0x00` and read again.
fn erase_experiment<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    order: &ProgramOrder,
    generation: u32,
    bufs: &mut Buffers,
    now_us: Clock<'_>,
    emit: Emit<'_>,
) -> Result<Outcome, F::Error> {
    let mut out = Outcome {
        dirty: SectorSet::default(),
        keep_next: [None; MAX_KEEP],
        torn: [TornWord::default(); MAX_TORN],
        n: 0,
        groups: [None; MAX_GROUPS],
    };
    let sectors = ERASE_SECTORS.min(order.sectors());
    let (mut first, mut first_class) = (None, "");
    let t0 = now_us();
    // Every sector the loop could have touched is checked for dirt (a
    // reset that interrupted some other loop leaves more), but only the
    // erase loop's own sectors are scanned for the one in flight.
    for k in 0..order.sectors() {
        let s = order.sector(k);
        let addr = layout.sector_addr(s);
        flash.read(addr, &mut bufs.read)?;
        if bufs.read.iter().all(|&b| b == 0xFF) {
            continue;
        }
        out.dirty.set(s);
        if first.is_none() && k < sectors {
            first = Some(k);
            fill(addr, generation, &mut bufs.and2);
            first_class = if bufs.read == bufs.and2 {
                "pattern"
            } else {
                "other"
            };
        }
    }
    let scan_us = now_us() - t0;
    emit(&EraseScanRecord {
        generation,
        sectors,
        first_dirty: first,
        first_class,
        scan_us,
    });
    // The sector in flight is the first not erased, or the last erased (a
    // torn erase that reads erased); the loop finished: the last.
    let candidates = match first {
        Some(0) => [Some(0), None],
        Some(k) => [Some(k - 1), Some(k)],
        None => [sectors.checked_sub(1), None],
    };
    for k in candidates.into_iter().flatten() {
        let s = order.sector(k);
        let addr = layout.sector_addr(s);
        window_sector(flash, addr, bufs)?;
        let w = classify_sector(
            &bufs.and,
            &bufs.or,
            Some(generation_pattern(addr, generation)),
        );
        if w.weak == 0 && (w.stable_zero == 0 || w.as_pattern == (SECTOR_SIZE * 8) as u32) {
            // Stably erased, or stably the old pattern: nothing torn here.
            continue;
        }
        let Some(slot) = out.keep_next.iter().position(Option::is_none) else {
            continue;
        };
        emit(&EraseSectorRecord {
            stage: "window",
            addr,
            index: k,
            reads: WINDOW_READS as u32,
            ..w
        });
        // The control: read left alone, against the window.
        sector_reads(flash, addr, CONTROL_CHUNKS, bufs, now_us)?;
        emit(&control_of(addr, bufs));
        // Program it to 0x00, what the erase's pre-program writes first.
        bufs.write.fill(0);
        let t0 = now_us();
        for w in 0..(SECTOR_SIZE / WRITE_SIZE) as u32 {
            flash.program(addr + w * WRITE_SIZE as u32, &bufs.write)?;
        }
        let us = now_us() - t0;
        emit(&ReprogramRecord {
            write: addr,
            span: SECTOR_SIZE as u32,
            us,
        });
        sector_reads(flash, addr, CONTROL_CHUNKS, bufs, now_us)?;
        let a = classify_sector(&bufs.and2, &bufs.or2, None);
        emit(&EraseSectorRecord {
            stage: "after",
            addr,
            index: k,
            reads: (SECTOR_CHUNK_READS * CONTROL_CHUNKS) as u32,
            ..a
        });
        out.keep_next[slot] = Some(s);
    }
    Ok(out)
}

/// The work loop: the region, write by write, in order, then nothing.
pub fn work_loop<F: ReprogramFlash>(
    flash: &mut F,
    armed: &Armed,
    buf: &mut [u8; WRITE_SIZE],
) -> Result<(), F::Error> {
    match mode_of(armed.generation) {
        LoopMode::Program => {
            let mut at = 0;
            while at < armed.order.len() {
                let addr = armed.order.addr(at);
                fill(addr, armed.generation, buf);
                flash.program(addr, buf)?;
                at += WRITE_SIZE as u32;
            }
        }
        LoopMode::Erase => {
            for k in 0..ERASE_SECTORS.min(armed.order.sectors()) {
                let addr = armed.order.layout.sector_addr(armed.order.sector(k));
                flash.erase(addr, addr + SECTOR_SIZE as u32)?;
            }
        }
    }
    Ok(())
}

/// Before an erase loop: program the sectors it will erase with its
/// generation's pattern (they are erased: the boot's prepare erased every
/// dirty sector). Returns how many; 0 before a program loop.
fn arm_erase_loop<F: ReprogramFlash>(
    flash: &mut F,
    armed: &Armed,
    bufs: &mut Buffers,
) -> Result<u32, F::Error> {
    if mode_of(armed.generation) != LoopMode::Erase {
        return Ok(0);
    }
    let sectors = ERASE_SECTORS.min(armed.order.sectors());
    for k in 0..sectors {
        let addr = armed.order.layout.sector_addr(armed.order.sector(k));
        for w in 0..(SECTOR_SIZE / WRITE_SIZE) as u32 {
            let at = addr + w * WRITE_SIZE as u32;
            fill(at, armed.generation, &mut bufs.write);
            flash.program(at, &bufs.write)?;
        }
    }
    Ok(sectors)
}

/// No state: erase the state sector and the region, arm generation 1.
fn fresh<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    bufs: &mut Buffers,
    facts: BootFacts<'_>,
    now_us: Clock<'_>,
    emit: Emit<'_>,
) -> Result<Armed, F::Error> {
    emit(&BootRecord {
        reset: facts.reset,
        mac: facts.mac,
        flash_id: facts.flash_id,
        base: layout.base,
        sectors: layout.sectors,
        fresh: true,
        seq: 0,
        generation: 0,
        keep: [None; MAX_KEEP],
        pending: 0,
        loop_mode: "none",
    });
    flash.erase(
        layout.state_addr(),
        layout.state_addr() + SECTOR_SIZE as u32,
    )?;
    let mut all = SectorSet::default();
    for s in REGION_FIRST..layout.sectors {
        all.set(s);
    }
    let (erased, erase_us, blank) = erase_dirty(flash, layout, &all, bufs, now_us)?;
    let next = State {
        seq: 0,
        generation: 1,
        ..State::default()
    };
    emit(&PrepareRecord {
        erased_sectors: erased + 1,
        erase_us,
        blank,
        keep: [None; MAX_KEEP],
        pending: 0,
        next_generation: next.generation,
        programmed_sectors: 0,
    });
    write_state(flash, layout, Some(0), &next)?;
    emit(&timing(flash, layout, bufs, now_us)?);
    Ok(Armed {
        order: ProgramOrder::new(*layout, &[]),
        generation: next.generation,
    })
}

/// Region sectors, a bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SectorSet([u32; MAX_SECTORS as usize / 32]);

impl SectorSet {
    fn set(&mut self, s: u32) {
        self.0[s as usize / 32] |= 1 << (s % 32);
    }
    fn clear(&mut self, s: u32) {
        self.0[s as usize / 32] &= !(1 << (s % 32));
    }
    fn has(&self, s: u32) -> bool {
        self.0[s as usize / 32] & (1 << (s % 32)) != 0
    }
}

struct Scan {
    frontier: Option<u32>,
    last_dirty: Option<u32>,
    /// From the frontier through the last dirty word: as meant, erased,
    /// neither.
    span: (u32, u32, u32),
    dirty: SectorSet,
}

fn scan_region<F: ReprogramFlash>(
    flash: &mut F,
    order: &ProgramOrder,
    generation: u32,
    bufs: &mut Buffers,
) -> Result<Scan, F::Error> {
    let mut out = Scan {
        frontier: None,
        last_dirty: None,
        span: (0, 0, 0),
        dirty: SectorSet::default(),
    };
    let mut counts = (0, 0, 0);
    for k in 0..order.sectors() {
        let sector = order.sector(k);
        let addr = order.layout.sector_addr(sector);
        flash.read(addr, &mut bufs.read)?;
        for (i, b) in bufs.read.chunks_exact(WORD_SIZE).enumerate() {
            let w = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let at = k * SECTOR_SIZE as u32 + (i * WORD_SIZE) as u32;
            let meant = intended_word(addr + (i * WORD_SIZE) as u32, generation);
            if out.frontier.is_none() && w != meant {
                out.frontier = Some(at);
            }
            if out.frontier.is_some() {
                if w == meant {
                    counts.0 += 1;
                } else if w == u32::MAX {
                    counts.1 += 1;
                } else {
                    counts.2 += 1;
                }
            }
            if w != u32::MAX {
                out.last_dirty = Some(at);
                out.dirty.set(sector);
                if out.frontier.is_some() {
                    out.span = counts;
                }
            }
        }
    }
    Ok(out)
}

struct Window {
    start: u32,
    end: u32,
    capped: bool,
    weak_words: u32,
    weak_bits: u32,
    found: usize,
    torn: [TornWord; MAX_TORN],
}

/// Read the writes around the frontier [`WINDOW_READS`] times into
/// `bufs.and`/`bufs.or`, and list the torn words.
fn window<F: ReprogramFlash>(
    flash: &mut F,
    order: &ProgramOrder,
    generation: u32,
    frontier: u32,
    last_dirty: Option<u32>,
    bufs: &mut Buffers,
) -> Result<Window, F::Error> {
    let ws = WRITE_SIZE as u32;
    let last = last_dirty.unwrap_or(0).max(frontier);
    let start = align_down(frontier, ws).saturating_sub(ws);
    let mut end = (align_down(last, ws) + 2 * ws).min(order.len());
    let capped = end - start > MAX_WINDOW as u32;
    if capped {
        end = start + MAX_WINDOW as u32;
    }
    let len = (end - start) as usize;
    for r in 0..WINDOW_READS {
        let mut at = start;
        while at < end {
            let o = (at - start) as usize;
            flash.read(order.addr(at), &mut bufs.read[..WRITE_SIZE])?;
            for i in 0..WRITE_SIZE {
                let b = bufs.read[i];
                if r == 0 {
                    bufs.and[o + i] = b;
                    bufs.or[o + i] = b;
                } else {
                    bufs.and[o + i] &= b;
                    bufs.or[o + i] |= b;
                }
            }
            at += ws;
        }
    }
    let mut w = Window {
        start,
        end,
        capped,
        weak_words: 0,
        weak_bits: 0,
        found: 0,
        torn: [TornWord::default(); MAX_TORN],
    };
    for o in (0..len).step_by(WORD_SIZE) {
        let at = start + o as u32;
        let addr = order.addr(at);
        let t = TornWord {
            addr,
            at,
            intended: intended_word(addr, generation),
            and: word_at(&bufs.and, o),
            or: word_at(&bufs.or, o),
        };
        let weak = t.weak();
        if weak != 0 {
            w.weak_words += 1;
            w.weak_bits += weak.count_ones();
        }
        if weak != 0 || (t.and != t.intended && t.and != u32::MAX) {
            if w.found < MAX_TORN {
                w.torn[w.found] = t;
            }
            w.found += 1;
        }
    }
    Ok(w)
}

/// The bytes to re-program in the write holding logical offset `at`: from
/// the write's start through its last word that did not read erased on
/// every window read.
fn span_of(window_start: u32, at: u32, and: &[u8]) -> u32 {
    let ws = WRITE_SIZE as u32;
    let write = align_down(at, ws);
    let o = (write - window_start) as usize;
    let mut span = (at - write) + WORD_SIZE as u32;
    for i in (0..WRITE_SIZE).step_by(WORD_SIZE) {
        if word_at(and, o + i) != u32::MAX {
            span = span.max((i + WORD_SIZE) as u32);
        }
    }
    span
}

/// Read one write [`WINDOW_READS`] times: its first `span` bytes should
/// hold what `generation` meant, the rest erased, all of it stable.
fn check_write<F: ReprogramFlash>(
    flash: &mut F,
    write: u32,
    span: u32,
    generation: u32,
    bufs: &mut Buffers,
) -> Result<WriteRecord<'static>, F::Error> {
    let mut and = [0u8; WRITE_SIZE];
    let mut or = [0u8; WRITE_SIZE];
    for r in 0..WINDOW_READS {
        flash.read(write, &mut bufs.read[..WRITE_SIZE])?;
        for i in 0..WRITE_SIZE {
            let b = bufs.read[i];
            if r == 0 {
                (and[i], or[i]) = (b, b);
            } else {
                and[i] &= b;
                or[i] |= b;
            }
        }
    }
    let mut rec = WriteRecord {
        stage: "",
        write,
        span,
        reads: WINDOW_READS,
        span_ok: true,
        tail_ok: true,
        weak_words: 0,
        wrong_words: 0,
    };
    for o in (0..WRITE_SIZE).step_by(WORD_SIZE) {
        let (a, b) = (word_at(&and, o), word_at(&or, o));
        let in_span = (o as u32) < span;
        let expect = if in_span {
            intended_word(write + o as u32, generation)
        } else {
            u32::MAX
        };
        let ok = a == b && a == expect;
        if a != b {
            rec.weak_words += 1;
        } else if a != expect {
            rec.wrong_words += 1;
        }
        if !ok {
            if in_span {
                rec.span_ok = false;
            } else {
                rec.tail_ok = false;
            }
        }
    }
    Ok(rec)
}

/// Read one word [`CONTROL_CHUNKS`] × [`CONTROL_CHUNK_READS`] times, the
/// chunks [`CONTROL_GAP_US`] apart.
fn read_word<F: ReprogramFlash>(
    flash: &mut F,
    addr: u32,
    intended: u32,
    now_us: Clock<'_>,
) -> Result<WordReads, F::Error> {
    let mut r = WordReads::new(addr, intended, CONTROL_CHUNK_READS as u32);
    let mut b = [0u8; WORD_SIZE];
    for c in 0..CONTROL_CHUNKS {
        if c > 0 {
            let t0 = now_us();
            while now_us() - t0 < CONTROL_GAP_US {}
        }
        for _ in 0..CONTROL_CHUNK_READS {
            flash.read(addr, &mut b)?;
            r.count(c, u32::from_le_bytes(b));
        }
    }
    Ok(r)
}

/// Erase the set's sectors in contiguous runs (esp-storage takes 64 KiB
/// blocks where a run allows), then read each back once.
fn erase_dirty<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    set: &SectorSet,
    bufs: &mut Buffers,
    now_us: Clock<'_>,
) -> Result<(u32, u64, bool), F::Error> {
    let (mut erased, mut us, mut blank) = (0, 0, true);
    let mut s = REGION_FIRST;
    while s < layout.sectors {
        if !set.has(s) {
            s += 1;
            continue;
        }
        let first = s;
        while s < layout.sectors && set.has(s) {
            s += 1;
        }
        let t0 = now_us();
        flash.erase(
            layout.sector_addr(first),
            layout.sector_addr(first) + (s - first) * SECTOR_SIZE as u32,
        )?;
        us += now_us() - t0;
        erased += s - first;
        for e in first..s {
            flash.read(layout.sector_addr(e), &mut bufs.read)?;
            blank &= bufs.read.iter().all(|&b| b == 0xFF);
        }
    }
    Ok((erased, us, blank))
}

fn write_state<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    free: Option<usize>,
    next: &State,
) -> Result<(), F::Error> {
    let slot = match free {
        Some(slot) => slot,
        None => {
            flash.erase(
                layout.state_addr(),
                layout.state_addr() + SECTOR_SIZE as u32,
            )?;
            0
        }
    };
    flash.program(
        layout.state_addr() + (slot * ENTRY_SIZE) as u32,
        &next.encode(),
    )
}

/// One erase and [`TIMING_WRITES`] writes in the scratch sector, timed.
fn timing<F: ReprogramFlash>(
    flash: &mut F,
    layout: &ReprogramLayout,
    bufs: &mut Buffers,
    now_us: Clock<'_>,
) -> Result<TimingRecord, F::Error> {
    let addr = layout.timing_addr();
    let mut t = TimingRecord {
        write_us_min: u64::MAX,
        ..TimingRecord::default()
    };
    let t0 = now_us();
    flash.erase(addr, addr + SECTOR_SIZE as u32)?;
    t.erase_us = now_us() - t0;
    for k in 0..TIMING_WRITES as u32 {
        let at = addr + k * WRITE_SIZE as u32;
        fill(at, 0, &mut bufs.write);
        let p0 = now_us();
        flash.program(at, &bufs.write)?;
        let dt = now_us() - p0;
        t.write_us_min = t.write_us_min.min(dt);
        t.write_us_max = t.write_us_max.max(dt);
        t.write_us_total += dt;
    }
    Ok(t)
}

/// Read the sector at `addr` [`WINDOW_READS`] times into `bufs.and`/`or`.
fn window_sector<F: ReprogramFlash>(
    flash: &mut F,
    addr: u32,
    bufs: &mut Buffers,
) -> Result<(), F::Error> {
    for r in 0..WINDOW_READS {
        flash.read(addr, &mut bufs.read)?;
        accumulate(&mut bufs.and, &mut bufs.or, &bufs.read, r == 0);
    }
    Ok(())
}

/// Read the sector at `addr` `chunks` × [`SECTOR_CHUNK_READS`] times, the
/// chunks [`CONTROL_GAP_US`] apart, into `bufs.and2`/`or2`.
fn sector_reads<F: ReprogramFlash>(
    flash: &mut F,
    addr: u32,
    chunks: usize,
    bufs: &mut Buffers,
    now_us: Clock<'_>,
) -> Result<(), F::Error> {
    for c in 0..chunks {
        if c > 0 {
            let t0 = now_us();
            while now_us() - t0 < CONTROL_GAP_US {}
        }
        for r in 0..SECTOR_CHUNK_READS {
            flash.read(addr, &mut bufs.read)?;
            accumulate(&mut bufs.and2, &mut bufs.or2, &bufs.read, c == 0 && r == 0);
        }
    }
    Ok(())
}

fn accumulate(and: &mut [u8], or: &mut [u8], read: &[u8], first: bool) {
    for i in 0..read.len() {
        if first {
            (and[i], or[i]) = (read[i], read[i]);
        } else {
            and[i] &= read[i];
            or[i] |= read[i];
        }
    }
}

/// A sector's bits by how they read; `pattern`: the old pattern's bytes, to
/// count the bits that still read as it.
fn classify_sector(and: &[u8], or: &[u8], pattern: Option<PatternOf>) -> EraseSectorRecord {
    let mut r = EraseSectorRecord::default();
    let mut leading = true;
    for i in 0..SECTOR_SIZE {
        let (a, o) = (and[i], or[i]);
        r.weak += (a ^ o).count_ones();
        r.stable_zero += (!o).count_ones();
        r.stable_one += a.count_ones();
        if leading && o == 0 {
            r.leading_zero_bytes += 1;
        } else {
            leading = false;
        }
        if let Some(p) = pattern {
            let pb = p.byte(i);
            // Stable bits equal to the pattern's.
            r.as_pattern += (!(a ^ o) & !(a ^ pb)).count_ones();
        }
    }
    r
}

/// The control's reads (`and2`/`or2`) against the window's (`and`/`or`).
fn control_of(addr: u32, bufs: &Buffers) -> EraseControlRecord {
    let mut r = EraseControlRecord {
        addr,
        reads: (SECTOR_CHUNK_READS * CONTROL_CHUNKS) as u32,
        weak_window: 0,
        weak_control: 0,
        weak_both: 0,
        settled_alone: 0,
        new_weak: 0,
        moved: 0,
    };
    for i in 0..SECTOR_SIZE {
        let ww = bufs.and[i] ^ bufs.or[i];
        let wc = bufs.and2[i] ^ bufs.or2[i];
        r.weak_window += ww.count_ones();
        r.weak_control += wc.count_ones();
        r.weak_both += (ww & wc).count_ones();
        r.settled_alone += (ww & !wc).count_ones();
        r.new_weak += (wc & !ww).count_ones();
        r.moved += (!ww & !wc & (bufs.and[i] ^ bufs.and2[i])).count_ones();
    }
    r
}

/// The old pattern of one sector, byte by byte.
#[derive(Clone, Copy)]
struct PatternOf {
    addr: u32,
    generation: u32,
}

impl PatternOf {
    fn byte(&self, i: usize) -> u8 {
        let w = intended_word(self.addr + (i & !3) as u32, self.generation);
        w.to_le_bytes()[i & 3]
    }
}

fn generation_pattern(addr: u32, generation: u32) -> PatternOf {
    PatternOf { addr, generation }
}

fn word_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

const fn align_down(x: u32, a: u32) -> u32 {
    x - x % a
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;
    use std::string::{String, ToString};
    use std::vec;
    use std::vec::Vec;

    use super::*;

    const BASE: u32 = 0x1_0000;
    /// State, timing and 18 region sectors.
    const SECTORS: u32 = 20;

    /// How the fake part tears the program the cut lands on.
    #[derive(Clone, Copy)]
    enum Tear {
        /// The first `n` bytes land, nothing after.
        Prefix(usize),
        /// The first `whole` bytes land; in the next word, bit `weak`
        /// (meant 0) is left weak and every other clear lands.
        WeakBit { whole: usize, weak: u32 },
        /// An erase caught lifting: the sector all 0 but for one stable 1
        /// (byte 500, bit 7) and three weak bits (byte 100 bit 4, byte
        /// 3000 bits 1 and 2).
        Erasing,
    }

    #[derive(Debug, PartialEq)]
    struct PowerLost;

    /// A NOR part: erase sets 1s, program clears; a weak bit reads 0 and 1
    /// on alternate reads. `settles`: programming a weak bit to 0 makes it a
    /// solid 0 (`lp-nor-sim`'s model); otherwise it stays weak.
    struct FakeNor {
        cells: Vec<u8>,
        weak: Vec<u8>,
        ops: u64,
        cut_after: Option<u64>,
        tear: Tear,
        powered: bool,
        flicker: u8,
        settles: bool,
    }

    impl FakeNor {
        fn new(settles: bool) -> Self {
            let len = (BASE + SECTORS * SECTOR_SIZE as u32) as usize;
            Self {
                cells: vec![0x00; len],
                weak: vec![0; len],
                ops: 0,
                cut_after: None,
                tear: Tear::Prefix(0),
                powered: true,
                flicker: 0,
                settles,
            }
        }
        /// Cut the `n`-th program from now.
        fn cut(&mut self, n: u64, tear: Tear) {
            self.ops = 0;
            self.cut_after = Some(n);
            self.tear = tear;
        }
        fn power_cycle(&mut self) {
            self.powered = true;
            self.cut_after = None;
        }
    }

    impl ReprogramFlash for FakeNor {
        type Error = PowerLost;
        fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), PowerLost> {
            if !self.powered {
                return Err(PowerLost);
            }
            self.flicker ^= 0xFF;
            let a = addr as usize;
            for (i, b) in buf.iter_mut().enumerate() {
                let w = self.weak[a + i];
                *b = (self.cells[a + i] & !w) | (self.flicker & w);
            }
            Ok(())
        }
        fn erase(&mut self, from: u32, to: u32) -> Result<(), PowerLost> {
            if !self.powered {
                return Err(PowerLost);
            }
            let hit = self.cut_after == Some(self.ops);
            self.ops += 1;
            let (a, b) = (from as usize, to as usize);
            if !hit {
                self.cells[a..b].fill(0xFF);
                self.weak[a..b].fill(0);
                return Ok(());
            }
            self.powered = false;
            if let Tear::Erasing = self.tear {
                self.cells[a..b].fill(0x00);
                self.cells[a + 500] = 0x80;
                self.weak[a + 100] |= 0x10;
                self.weak[a + 3000] |= 0x06;
            }
            Err(PowerLost)
        }
        fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), PowerLost> {
            if !self.powered {
                return Err(PowerLost);
            }
            let a = addr as usize;
            let hit = self.cut_after == Some(self.ops);
            self.ops += 1;
            let n = match (hit, self.tear) {
                (false, _) => data.len(),
                (true, Tear::Prefix(n)) => n.min(data.len()),
                (true, Tear::WeakBit { whole, .. }) => whole.min(data.len()),
                (true, Tear::Erasing) => 0,
            };
            for i in 0..n {
                self.cells[a + i] &= data[i];
                // Settling: a weak bit programmed to 0 is a solid 0.
                // Otherwise it stays weak, whatever it is programmed with.
                if self.settles {
                    self.weak[a + i] &= data[i];
                }
            }
            if !hit {
                return Ok(());
            }
            self.powered = false;
            if let Tear::WeakBit { whole, weak } = self.tear {
                // The next word: every clear but `weak` lands; `weak` is
                // left half-programmed.
                for i in whole..(whole + 4).min(data.len()) {
                    let bit_here = |b: u32| (b / 8) as usize == i - whole;
                    let mut d = data[i];
                    if bit_here(weak) {
                        let m = 1u8 << (weak % 8);
                        if data[i] & m == 0 {
                            self.weak[a + i] |= m;
                        }
                        d |= m;
                    }
                    self.cells[a + i] &= d;
                }
            }
            Err(PowerLost)
        }
    }

    fn layout() -> ReprogramLayout {
        ReprogramLayout::new(BASE, SECTORS * SECTOR_SIZE as u32).unwrap()
    }

    fn boot_once(nor: &mut FakeNor) -> (Vec<String>, Armed) {
        let mut bufs = Box::new(Buffers::new());
        let mut out = Vec::new();
        let mut emit = |r: &dyn fmt::Display| out.push(r.to_string());
        let mut clock = 0u64;
        let mut now = || {
            clock += 9;
            clock
        };
        let facts = BootFacts {
            reset: "poweron",
            mac: [0; 6],
            flash_id: 0,
        };
        let armed = boot(nor, &layout(), &mut bufs, facts, &mut now, &mut emit).unwrap();
        (out, armed)
    }

    /// Run the loop until the cut set on `nor` fires (or the region fills).
    fn work(nor: &mut FakeNor, armed: &Armed) {
        let mut buf = [0u8; WRITE_SIZE];
        let _ = work_loop(nor, armed, &mut buf);
        nor.power_cycle();
    }

    fn records<'a>(out: &'a [String], kind: &str) -> Vec<&'a str> {
        let tag = std::format!("\"kind\":\"{kind}\"");
        out.iter()
            .filter(|r| r.contains(&tag))
            .map(String::as_str)
            .collect()
    }

    /// The first clear (meant-0 bit) of the word at byte `whole` of write
    /// `k` of the region under `armed`.
    fn first_clear(armed: &Armed, k: u32, whole: usize) -> u32 {
        let addr = armed.order.addr(k * WRITE_SIZE as u32) + whole as u32;
        let w = intended_word(addr, armed.generation);
        (!w).trailing_zeros()
    }

    #[test]
    fn a_fresh_region_is_erased_and_armed() {
        let mut nor = FakeNor::new(true);
        let (out, armed) = boot_once(&mut nor);
        assert!(out[0].contains(r#""state":"fresh""#), "{}", out[0]);
        let prep = records(&out, "fr-prepare")[0];
        assert!(prep.contains(r#""erased_sectors":19"#), "{prep}");
        assert!(prep.contains(r#""blank":true"#), "{prep}");
        assert_eq!(armed.generation, 1);
        assert_eq!(armed.order.len(), 18 * SECTOR_SIZE as u32);
        assert_eq!(records(&out, "fr-timing").len(), 1);
    }

    #[test]
    fn a_region_the_loop_filled_has_no_frontier() {
        let mut nor = FakeNor::new(true);
        let (_, armed) = boot_once(&mut nor);
        work(&mut nor, &armed);
        let (out, _) = boot_once(&mut nor);
        let scan = records(&out, "fr-scan")[0];
        assert!(scan.contains(r#""frontier":null"#), "{scan}");
        assert!(records(&out, "fr-window").is_empty());
        let prep = records(&out, "fr-prepare")[0];
        assert!(prep.contains(r#""erased_sectors":18"#), "{prep}");
    }

    #[test]
    fn a_clean_cut_between_words_has_no_torn_word_and_reprograms_nothing() {
        let mut nor = FakeNor::new(true);
        let (_, armed) = boot_once(&mut nor);
        nor.cut(37, Tear::Prefix(96));
        work(&mut nor, &armed);
        let (out, armed) = boot_once(&mut nor);
        let scan = records(&out, "fr-scan")[0];
        assert!(
            scan.contains(&std::format!(r#""frontier":{}"#, 37 * 256 + 96)),
            "{scan}"
        );
        assert!(scan.contains(r#""span_words":0"#), "{scan}");
        let win = records(&out, "fr-window")[0];
        assert!(win.contains(r#""torn_words":0"#), "{win}");
        assert!(records(&out, "fr-reprogram").is_empty());
        // Nothing kept: the whole region is the next loop's.
        assert_eq!(armed.order.len(), 18 * SECTOR_SIZE as u32);
    }

    #[test]
    fn a_weak_bit_is_found_controlled_reprogrammed_and_rechecked_on_a_settling_part() {
        let mut nor = FakeNor::new(true);
        let (_, armed) = boot_once(&mut nor);
        // Write 40 is in the loop's third sector, 2,048 bytes in: 64 bytes
        // land, the next word tears.
        let weak = first_clear(&armed, 40, 64);
        nor.cut(40, Tear::WeakBit { whole: 64, weak });
        work(&mut nor, &armed);
        let (out, next) = boot_once(&mut nor);
        let win = records(&out, "fr-window")[0];
        assert!(
            win.contains(r#""weak_words":1,"weak_bits":1,"torn_words":1,"followed":1"#),
            "{win}"
        );
        let torn = records(&out, "fr-torn");
        assert_eq!(torn.len(), 1);
        assert!(torn[0].contains(r#""in_write":64"#), "{}", torn[0]);
        assert!(
            torn[0].contains(r#""weak":1,"weak_clears":1,"weak_ones":0"#),
            "{}",
            torn[0]
        );
        assert!(torn[0].contains(r#""unlanded":0"#), "{}", torn[0]);
        let reads = records(&out, "fr-reads");
        assert_eq!(reads.len(), 2);
        assert!(reads[0].contains(r#""stage":"before""#) && reads[0].contains(r#""stable":false"#));
        assert!(
            reads[0].contains(&std::format!(r#""bits":[{{"bit":{weak},"zeros":[256,"#)),
            "{}",
            reads[0]
        );
        assert!(reads[1].contains(r#""stage":"after""#) && reads[1].contains(r#""correct":true"#));
        let rp = records(&out, "fr-reprogram")[0];
        assert!(rp.contains(r#""span":68"#), "{rp}");
        let wr = records(&out, "fr-write")[0];
        assert!(
            wr.contains(r#""stage":"after""#) && wr.contains(r#""span_ok":true,"tail_ok":true"#),
            "{wr}"
        );
        // The torn word's sector is kept out of the next loop.
        let kept = armed.order.sector(2);
        assert_eq!(next.order.keep().collect::<Vec<_>>(), [kept]);
        assert_eq!(next.order.sectors(), 17);

        // The next cut is the further power cycle; the word is re-checked.
        nor.cut(3, Tear::Prefix(0));
        work(&mut nor, &next);
        let (out, last) = boot_once(&mut nor);
        assert!(out[0].contains(r#""pending":1"#), "{}", out[0]);
        let re = records(&out, "fr-reads");
        assert!(
            re[0].contains(r#""stage":"recheck""#) && re[0].contains(r#""correct":true"#),
            "{}",
            re[0]
        );
        let wr = records(&out, "fr-write")[0];
        assert!(
            wr.contains(r#""stage":"recheck""#) && wr.contains(r#""span_ok":true,"tail_ok":true"#),
            "{wr}"
        );
        // And then released.
        assert!(last.order.keep().next().is_none());
        let prep = records(&out, "fr-prepare")[0];
        assert!(prep.contains(r#""blank":true"#), "{prep}");
    }

    #[test]
    fn a_part_whose_weak_bits_do_not_settle_is_reported_as_such() {
        let mut nor = FakeNor::new(false);
        let (_, armed) = boot_once(&mut nor);
        let weak = first_clear(&armed, 5, 160);
        nor.cut(5, Tear::WeakBit { whole: 160, weak });
        work(&mut nor, &armed);
        let (out, next) = boot_once(&mut nor);
        let reads = records(&out, "fr-reads");
        assert!(
            reads[1].contains(r#""stage":"after""#) && reads[1].contains(r#""stable":false"#),
            "{}",
            reads[1]
        );
        let wr = records(&out, "fr-write")[0];
        assert!(
            wr.contains(r#""span_ok":false"#) && wr.contains(r#""weak_words":1"#),
            "{wr}"
        );
        nor.cut(0, Tear::Prefix(0));
        work(&mut nor, &next);
        let (out, _) = boot_once(&mut nor);
        let re = records(&out, "fr-reads");
        assert!(
            re[0].contains(r#""stage":"recheck""#) && re[0].contains(r#""correct":false"#),
            "{}",
            re[0]
        );
    }

    #[test]
    fn a_kept_sector_is_skipped_by_the_loop_and_untouched_by_its_cut() {
        let mut nor = FakeNor::new(true);
        let (_, armed) = boot_once(&mut nor);
        let weak = first_clear(&armed, 2, 4);
        nor.cut(2, Tear::WeakBit { whole: 4, weak });
        work(&mut nor, &armed);
        let (_, next) = boot_once(&mut nor);
        let kept = next.order.keep().next().unwrap();
        assert_eq!(kept, REGION_FIRST);
        let snapshot = nor.cells[layout().sector_addr(kept) as usize..][..SECTOR_SIZE].to_vec();
        // A full loop programs every sector but the kept one.
        work(&mut nor, &next);
        assert_eq!(
            &nor.cells[layout().sector_addr(kept) as usize..][..SECTOR_SIZE],
            &snapshot[..]
        );
        let (out, _) = boot_once(&mut nor);
        // Generation 2's loop erased.
        assert!(out[0].contains(r#""loop":"erase""#), "{}", out[0]);
        let scan = records(&out, "fr-escan")[0];
        assert!(scan.contains(r#""first_dirty":null"#), "{scan}");
        assert!(records(&out, "fr-reads")[0].contains(r#""correct":true"#));
    }

    /// Generation 1 programs; generation 2's loop erases what its boot
    /// programmed.
    fn to_an_erase_loop(nor: &mut FakeNor) -> Armed {
        let (_, armed) = boot_once(nor);
        assert_eq!(armed.generation, 1);
        work(nor, &armed);
        let (out, armed) = boot_once(nor);
        let prep = records(&out, "fr-prepare")[0];
        assert!(
            prep.contains(r#""next_loop":"erase","programmed_sectors":18"#),
            "{prep}"
        );
        assert_eq!(mode_of(armed.generation), LoopMode::Erase);
        armed
    }

    #[test]
    fn a_torn_erase_is_read_left_alone_zeroed_and_rechecked_on_a_settling_part() {
        let mut nor = FakeNor::new(true);
        let armed = to_an_erase_loop(&mut nor);
        // Five sectors erased, the sixth torn.
        nor.cut(5, Tear::Erasing);
        work(&mut nor, &armed);
        let (out, next) = boot_once(&mut nor);
        assert!(out[0].contains(r#""loop":"erase""#), "{}", out[0]);
        let scan = records(&out, "fr-escan")[0];
        assert!(
            scan.contains(r#""first_dirty":5,"first_class":"other""#),
            "{scan}"
        );
        let sectors = records(&out, "fr-esector");
        assert_eq!(sectors.len(), 2, "{sectors:?}");
        assert!(sectors[0].contains(r#""stage":"window""#), "{}", sectors[0]);
        assert!(sectors[0].contains(r#""weak":3,"#), "{}", sectors[0]);
        assert!(sectors[0].contains(r#""stable_one":1,"#), "{}", sectors[0]);
        let ctl = records(&out, "fr-econtrol")[0];
        assert!(
            ctl.contains(r#""weak_window":3,"weak_control":3,"weak_both":3,"settled_alone":0"#),
            "{ctl}"
        );
        assert!(
            sectors[1].contains(r#""stage":"after""#) && sectors[1].contains(r#""zeroed":true"#),
            "{}",
            sectors[1]
        );
        let kept = armed.order.sector(5);
        assert_eq!(next.order.keep().collect::<Vec<_>>(), [kept]);
        assert_eq!(mode_of(next.generation), LoopMode::Program);

        // The next cut is the further power cycle.
        nor.cut(7, Tear::Prefix(0));
        work(&mut nor, &next);
        let (out, last) = boot_once(&mut nor);
        let re = records(&out, "fr-esector");
        assert!(
            re[0].contains(r#""stage":"recheck""#) && re[0].contains(r#""zeroed":true"#),
            "{}",
            re[0]
        );
        assert!(last.order.keep().next().is_none());
        assert!(records(&out, "fr-prepare")[0].contains(r#""blank":true"#));
    }

    #[test]
    fn a_torn_erase_whose_weak_bits_do_not_settle_reads_weak_after_zeroing() {
        let mut nor = FakeNor::new(false);
        let armed = to_an_erase_loop(&mut nor);
        nor.cut(0, Tear::Erasing);
        work(&mut nor, &armed);
        let (out, _) = boot_once(&mut nor);
        let scan = records(&out, "fr-escan")[0];
        assert!(scan.contains(r#""first_dirty":0"#), "{scan}");
        let after = records(&out, "fr-esector")[1];
        assert!(
            after.contains(r#""stage":"after""#) && after.contains(r#""weak":3,"#),
            "{after}"
        );
        assert!(after.contains(r#""zeroed":false"#), "{after}");
    }

    #[test]
    fn an_erase_that_did_not_start_or_finished_whole_is_not_torn() {
        let mut nor = FakeNor::new(true);
        let armed = to_an_erase_loop(&mut nor);
        // The cut lands on the fourth erase, which does nothing.
        nor.cut(3, Tear::Prefix(0));
        work(&mut nor, &armed);
        let (out, next) = boot_once(&mut nor);
        let scan = records(&out, "fr-escan")[0];
        assert!(
            scan.contains(r#""first_dirty":3,"first_class":"pattern""#),
            "{scan}"
        );
        assert!(records(&out, "fr-esector").is_empty());
        assert!(next.order.keep().next().is_none());
        // Every dirty sector erased for the program loop.
        let prep = records(&out, "fr-prepare")[0];
        assert!(
            prep.contains(r#""erased_sectors":15"#) && prep.contains(r#""blank":true"#),
            "{prep}"
        );
    }
}
