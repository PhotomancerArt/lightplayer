//! The payload's flow, over an injected flash: scan, repair, one timed cycle,
//! and the silent work loop.
//!
//! The firmware supplies a [`TearsFlash`] (esp-storage on the C6) and a
//! microsecond clock; everything else is here so that the whole boot can be
//! run on the host against a fake part with cuts in it (the tests below).

use core::fmt;

use super::analysis::{Verdict, accumulate, analyze_in_flight, analyze_settled};
use super::journal::{self, CopyScan};
use super::layout::{TearsLayout, last_cycle_of, sector_of};
use super::pattern::fill_pattern;
use super::records::{
    BootRecord, InFlightRecord, JournalRecord, RepairRecord, SettledRecord, SummaryRecord,
    TimingRecord,
};
use super::{JOURNAL_COPIES, PAGE_SIZE, REGION_SECTORS, SCAN_READS, SECTOR_SIZE};

/// The flash the payload drives. Addresses are absolute.
pub trait TearsFlash {
    type Error: fmt::Debug;
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Self::Error>;
    /// Erase the 4 KiB sector at `addr`.
    fn erase_sector(&mut self, addr: u32) -> Result<(), Self::Error>;
    /// Program `data` at `addr`. The runner never crosses a page.
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), Self::Error>;
}

/// Where a record goes: the firmware prints it behind `[fw-check-json] `.
pub type Emit<'a> = &'a mut dyn FnMut(&dyn fmt::Display);

/// Working memory for one boot: five sectors' worth, 20 KiB. The firmware
/// puts it on the heap; nothing here allocates.
pub struct ScanBuffers {
    pub and: [u8; SECTOR_SIZE],
    pub or: [u8; SECTOR_SIZE],
    pub read: [u8; SECTOR_SIZE],
    pub old: [u8; SECTOR_SIZE],
    pub new: [u8; SECTOR_SIZE],
}

impl ScanBuffers {
    pub const fn new() -> Self {
        Self {
            and: [0; SECTOR_SIZE],
            or: [0; SECTOR_SIZE],
            read: [0; SECTOR_SIZE],
            old: [0; SECTOR_SIZE],
            new: [0; SECTOR_SIZE],
        }
    }
}

impl Default for ScanBuffers {
    fn default() -> Self {
        Self::new()
    }
}

/// What the scan found that the repair needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BootScan {
    /// The in-flight cycle, or `None` on a fresh region.
    pub latest: Option<u32>,
    /// Region sectors (bit per sector) that do not hold what their last
    /// cycle meant them to.
    pub needs_rewrite: u32,
    /// Journal copies (bit per copy) holding a damaged slot.
    pub journal_damaged: u32,
    /// The in-flight sector's verdict, when there was one.
    pub in_flight: Option<Verdict>,
}

/// Who is booting: the facts the firmware knows about the board and the part.
#[derive(Clone, Copy, Debug)]
pub struct BootFacts<'a> {
    /// The reset reason, as the firmware names it (`poweron` after a cut).
    pub reset: &'a str,
    /// The chip's base MAC.
    pub mac: [u8; 6],
    /// The flash part's JEDEC id.
    pub flash_id: u32,
}

/// Read the journal and every region sector, emit a record for each, and say
/// what needs rewriting. Writes nothing.
pub fn scan<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    bufs: &mut ScanBuffers,
    facts: BootFacts<'_>,
    emit: Emit<'_>,
) -> Result<BootScan, F::Error> {
    let mut copies = [CopyScan::default(); JOURNAL_COPIES as usize];
    for (k, copy) in copies.iter_mut().enumerate() {
        flash.read(layout.journal_addr(k as u32), &mut bufs.read)?;
        *copy = journal::scan_copy(k as u32, &bufs.read);
    }
    let latest = journal::latest_of(&copies);
    emit(&BootRecord {
        reset: facts.reset,
        mac: facts.mac,
        flash_id: facts.flash_id,
        base: layout.base,
        latest,
    });
    let mut out = BootScan {
        latest,
        ..BootScan::default()
    };
    for (k, copy) in copies.iter().enumerate() {
        emit(&JournalRecord {
            copy: k as u32,
            scan: *copy,
        });
        if copy.damaged > 0 {
            out.journal_damaged |= 1 << k;
        }
    }
    let Some(latest) = latest else {
        return Ok(out);
    };

    let in_flight_sector = sector_of(latest);
    let (mut complete, mut unexpected, mut weak_sectors) = (0, 0, 0);
    for sector in 0..REGION_SECTORS {
        let Some(wrote) = last_cycle_of(sector, latest) else {
            // A region the init pass never finished (a cut before the first
            // scan-done, which the host never makes): nothing to expect.
            out.needs_rewrite |= 1 << sector;
            continue;
        };
        let addr = layout.sector_addr(sector);
        for r in 0..SCAN_READS {
            flash.read(addr, &mut bufs.read)?;
            accumulate(&mut bufs.and, &mut bufs.or, &bufs.read, r == 0);
        }
        if sector == in_flight_sector && wrote >= REGION_SECTORS {
            fill_pattern(sector, wrote - REGION_SECTORS, &mut bufs.old);
            fill_pattern(sector, wrote, &mut bufs.new);
            let f = analyze_in_flight(&bufs.and, &bufs.or, &bufs.old, &bufs.new);
            if f.weak_bits > 0 {
                weak_sectors += 1;
            }
            if f.verdict != Verdict::Complete {
                out.needs_rewrite |= 1 << sector;
            }
            out.in_flight = Some(f.verdict);
            emit(&InFlightRecord { latest, sector, f });
        } else {
            fill_pattern(sector, wrote, &mut bufs.new);
            let s = analyze_settled(&bufs.and, &bufs.or, &bufs.new);
            if s.weak_bits > 0 {
                weak_sectors += 1;
            }
            if s.complete {
                complete += 1;
            } else {
                unexpected += 1;
                out.needs_rewrite |= 1 << sector;
            }
            emit(&SettledRecord {
                latest,
                sector,
                wrote,
                settled: s,
            });
        }
    }
    emit(&SummaryRecord {
        latest,
        in_flight_sector,
        in_flight: out.in_flight.map_or("none", Verdict::name),
        settled_complete: complete,
        settled_unexpected: unexpected,
        weak_sectors,
    });
    Ok(out)
}

/// Put the region back into a state the next scan can judge, and say where
/// the work loop resumes.
///
/// A fresh region gets a full pass of work cycles (so every sector holds a
/// known pattern before the host is allowed to cut); a resumed one gets its
/// damaged sectors rewritten and its damaged journal copies replaced.
pub fn prepare<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    found: &BootScan,
    bufs: &mut ScanBuffers,
    emit: Emit<'_>,
) -> Result<u32, F::Error> {
    let mut repair = RepairRecord {
        sectors: 0,
        journal_copies: 0,
        init_cycles: 0,
        next: 0,
    };
    let next = match found.latest {
        None => {
            // Both copies blank first: copy 1's first wrap is 128 cycles
            // away, and whatever the partition held before is not a journal.
            for copy in 0..JOURNAL_COPIES {
                flash.erase_sector(layout.journal_addr(copy))?;
            }
            run_cycles(flash, layout, 0, REGION_SECTORS, &mut bufs.new)?;
            repair.init_cycles = REGION_SECTORS;
            REGION_SECTORS
        }
        Some(latest) => {
            for copy in 0..JOURNAL_COPIES {
                if found.journal_damaged & (1 << copy) != 0 {
                    let addr = layout.journal_addr(copy);
                    flash.erase_sector(addr)?;
                    let slot = journal::slot_of(copy, latest) as usize * journal::ENTRY_SIZE;
                    flash.program(addr + slot as u32, &journal::encode(latest))?;
                    repair.journal_copies += 1;
                }
            }
            for sector in 0..REGION_SECTORS {
                if found.needs_rewrite & (1 << sector) == 0 {
                    continue;
                }
                if let Some(wrote) = last_cycle_of(sector, latest) {
                    write_sector(flash, layout, sector, wrote, &mut bufs.new)?;
                    repair.sectors += 1;
                }
            }
            // A region whose init pass was cut short finishes it now.
            let mut next = latest + 1;
            if next < REGION_SECTORS {
                let more = REGION_SECTORS - next;
                run_cycles(flash, layout, next, more, &mut bufs.new)?;
                repair.init_cycles = more;
                next = REGION_SECTORS;
            }
            next
        }
    };
    repair.next = next;
    emit(&repair);
    Ok(next)
}

/// Run one work cycle and time its three parts with `now_us`.
pub fn timed_cycle<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    cycle: u32,
    buf: &mut [u8; SECTOR_SIZE],
    now_us: &mut dyn FnMut() -> u64,
) -> Result<TimingRecord, F::Error> {
    let mut t = TimingRecord {
        cycle,
        page_us_min: u64::MAX,
        ..TimingRecord::default()
    };
    let t0 = now_us();
    write_journal(flash, layout, cycle)?;
    let t1 = now_us();
    let sector = sector_of(cycle);
    let addr = layout.sector_addr(sector);
    flash.erase_sector(addr)?;
    let t2 = now_us();
    fill_pattern(sector, cycle, buf);
    for (i, page) in buf.chunks(PAGE_SIZE).enumerate() {
        let p0 = now_us();
        flash.program(addr + (i * PAGE_SIZE) as u32, page)?;
        let dt = now_us() - p0;
        t.page_us_min = t.page_us_min.min(dt);
        t.page_us_max = t.page_us_max.max(dt);
    }
    let t3 = now_us();
    t.journal_us = t1 - t0;
    t.erase_us = t2 - t1;
    t.program_us = t3 - t2;
    Ok(t)
}

/// Run `count` work cycles from `first`, silently. Returns the next cycle.
pub fn run_cycles<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    first: u32,
    count: u32,
    buf: &mut [u8; SECTOR_SIZE],
) -> Result<u32, F::Error> {
    for cycle in first..first + count {
        run_cycle(flash, layout, cycle, buf)?;
    }
    Ok(first + count)
}

/// One work cycle: journal both copies, erase the cycle's sector, program it
/// page by page.
pub fn run_cycle<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    cycle: u32,
    buf: &mut [u8; SECTOR_SIZE],
) -> Result<(), F::Error> {
    write_journal(flash, layout, cycle)?;
    let sector = sector_of(cycle);
    write_sector(flash, layout, sector, cycle, buf)
}

fn write_journal<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    cycle: u32,
) -> Result<(), F::Error> {
    let entry = journal::encode(cycle);
    for copy in 0..JOURNAL_COPIES {
        let addr = layout.journal_addr(copy);
        let slot = journal::slot_of(copy, cycle);
        if slot == 0 {
            flash.erase_sector(addr)?;
        }
        flash.program(addr + slot * journal::ENTRY_SIZE as u32, &entry)?;
    }
    Ok(())
}

fn write_sector<F: TearsFlash>(
    flash: &mut F,
    layout: &TearsLayout,
    sector: u32,
    cycle: u32,
    buf: &mut [u8; SECTOR_SIZE],
) -> Result<(), F::Error> {
    let addr = layout.sector_addr(sector);
    flash.erase_sector(addr)?;
    fill_pattern(sector, cycle, buf);
    for (i, page) in buf.chunks(PAGE_SIZE).enumerate() {
        flash.program(addr + (i * PAGE_SIZE) as u32, page)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;
    use std::string::{String, ToString};
    use std::vec;
    use std::vec::Vec;

    use super::*;
    use crate::checks::flash_tears::LAYOUT_SECTORS;

    /// How the fake part tears the op the cut lands on.
    #[derive(Clone, Copy)]
    enum Tear {
        /// The op does nothing.
        Clean,
        /// An erase: the first half of the sector erased, the rest old.
        HalfErase,
        /// An erase: reads all `0xFF` but bit 0 of byte 7 is weak.
        WeakErase,
        /// A program: the first `n` bytes land.
        Prefix(usize),
    }

    #[derive(Debug, PartialEq)]
    struct PowerLost;

    /// A NOR part: erase sets 1s, program clears, a cut after `cut_after`
    /// ops tears that op and refuses everything after it.
    struct FakeNor {
        cells: Vec<u8>,
        weak: Vec<u8>,
        ops: u64,
        cut_after: Option<u64>,
        tear: Tear,
        powered: bool,
        flicker: u8,
    }

    const BASE: u32 = 0x1_0000;

    impl FakeNor {
        fn new() -> Self {
            let len = BASE as usize + LAYOUT_SECTORS as usize * SECTOR_SIZE;
            Self {
                cells: vec![0x00; len],
                weak: vec![0; len],
                ops: 0,
                cut_after: None,
                tear: Tear::Clean,
                powered: true,
                flicker: 0,
            }
        }
        fn cut(&mut self, after: u64, tear: Tear) {
            self.ops = 0;
            self.cut_after = Some(after);
            self.tear = tear;
        }
        fn power_cycle(&mut self) {
            self.powered = true;
            self.cut_after = None;
        }
        /// Is this op the one the cut lands on?
        fn tears(&mut self) -> Result<bool, PowerLost> {
            if !self.powered {
                return Err(PowerLost);
            }
            let hit = self.cut_after == Some(self.ops);
            self.ops += 1;
            if hit {
                self.powered = false;
            }
            Ok(hit)
        }
    }

    impl TearsFlash for FakeNor {
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
        fn erase_sector(&mut self, addr: u32) -> Result<(), PowerLost> {
            let a = addr as usize;
            let torn = self.tears()?;
            let cells = &mut self.cells[a..a + SECTOR_SIZE];
            if !torn {
                cells.fill(0xFF);
                self.weak[a..a + SECTOR_SIZE].fill(0);
                return Ok(());
            }
            match self.tear {
                Tear::Clean | Tear::Prefix(_) => {}
                Tear::HalfErase => cells[..SECTOR_SIZE / 2].fill(0xFF),
                Tear::WeakErase => {
                    cells.fill(0xFF);
                    self.weak[a + 7] = 0x01;
                }
            }
            Err(PowerLost)
        }
        fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), PowerLost> {
            let a = addr as usize;
            let n = match (self.tears()?, self.tear) {
                (false, _) => data.len(),
                (true, Tear::Prefix(n)) => n.min(data.len()),
                (true, _) => 0,
            };
            for i in 0..n {
                self.cells[a + i] &= data[i];
            }
            if n < data.len() {
                Err(PowerLost)
            } else {
                Ok(())
            }
        }
    }

    /// One boot: scan, repair, a timed cycle. Returns the records and the
    /// cycle the loop resumes at.
    fn boot(nor: &mut FakeNor) -> (Vec<String>, u32) {
        let layout = TearsLayout::new(BASE, LAYOUT_SECTORS * SECTOR_SIZE as u32).unwrap();
        let mut bufs = Box::new(ScanBuffers::new());
        let mut out: Vec<String> = Vec::new();
        let mut emit = |r: &dyn fmt::Display| out.push(r.to_string());
        let facts = BootFacts {
            reset: "poweron",
            mac: [0; 6],
            flash_id: 0,
        };
        let found = scan(nor, &layout, &mut bufs, facts, &mut emit).unwrap();
        let next = prepare(nor, &layout, &found, &mut bufs, &mut emit).unwrap();
        let mut clock = 0u64;
        let mut now = || {
            clock += 7;
            clock
        };
        let t = timed_cycle(nor, &layout, next, &mut bufs.new, &mut now).unwrap();
        emit(&t);
        (out, next + 1)
    }

    /// Run the silent loop from `next` until the cut fires.
    fn work_until_cut(nor: &mut FakeNor, next: u32) {
        let layout = TearsLayout::new(BASE, LAYOUT_SECTORS * SECTOR_SIZE as u32).unwrap();
        let mut buf = Box::new([0u8; SECTOR_SIZE]);
        let r = run_cycles(nor, &layout, next, 1000, &mut buf);
        assert_eq!(r, Err(PowerLost), "the cut must land inside the loop");
        nor.power_cycle();
    }

    fn in_flight(records: &[String]) -> &str {
        records
            .iter()
            .find(|r| r.contains(r#""role":"in-flight""#))
            .map(String::as_str)
            .expect("an in-flight record")
    }

    #[test]
    fn a_fresh_region_is_filled_before_the_first_scan_done() {
        let mut nor = FakeNor::new();
        let (records, next) = boot(&mut nor);
        assert!(records[0].contains(r#""state":"fresh""#), "{}", records[0]);
        assert!(records.iter().any(|r| r.contains(r#""init_cycles":16"#)));
        assert_eq!(next, REGION_SECTORS + 1);
        // The boot after a clean restart sees every sector whole.
        let (records, _) = boot(&mut nor);
        assert!(records[0].contains(r#""latest":16"#), "{}", records[0]);
        let summary = records.iter().find(|r| r.contains("ft-summary")).unwrap();
        assert!(summary.contains(r#""in_flight":"complete""#), "{summary}");
        assert!(summary.contains(r#""settled_complete":15"#), "{summary}");
    }

    /// Each cycle is 2 journal programs + 1 erase + 16 page programs = 19
    /// ops (plus a journal erase at a wrap). Offsets inside a cycle:
    /// 0,1 journal, 2 erase, 3.. pages.
    const CYCLE_OPS: u64 = 19;

    #[test]
    fn a_cut_in_the_erase_is_a_torn_erase_and_is_repaired() {
        let mut nor = FakeNor::new();
        let (_, next) = boot(&mut nor);
        nor.cut(CYCLE_OPS * 3 + 2, Tear::HalfErase);
        work_until_cut(&mut nor, next);
        let (records, _) = boot(&mut nor);
        let f = in_flight(&records);
        assert!(f.contains(r#""verdict":"torn-erase""#), "{f}");
        assert!(f.contains(r#""leading_ff_bytes":2048"#), "{f}");
        assert!(records.iter().any(|r| r.contains(r#""sectors":1"#)));
        // Repaired: the next boot sees nothing wrong but the new in-flight.
        let (records, _) = boot(&mut nor);
        let summary = records.iter().find(|r| r.contains("ft-summary")).unwrap();
        assert!(summary.contains(r#""settled_unexpected":0"#), "{summary}");
    }

    #[test]
    fn an_erase_that_reads_erased_but_flickers_is_erased_weak() {
        let mut nor = FakeNor::new();
        let (_, next) = boot(&mut nor);
        nor.cut(CYCLE_OPS + 2, Tear::WeakErase);
        work_until_cut(&mut nor, next);
        let (records, _) = boot(&mut nor);
        let f = in_flight(&records);
        assert!(f.contains(r#""verdict":"erased-weak""#), "{f}");
        assert!(f.contains(r#""weak_bits":1,"#), "{f}");
    }

    #[test]
    fn a_cut_in_a_page_program_is_a_torn_program_with_its_shape() {
        let mut nor = FakeNor::new();
        let (_, next) = boot(&mut nor);
        // Page 5 of the cycle, 40 of its bytes landed.
        nor.cut(CYCLE_OPS * 2 + 3 + 5, Tear::Prefix(40));
        work_until_cut(&mut nor, next);
        let (records, _) = boot(&mut nor);
        let f = in_flight(&records);
        assert!(f.contains(r#""verdict":"torn-program""#), "{f}");
        assert!(f.contains(r#""shape":"byte-prefix""#), "{f}");
        // Pages 0..5 whole, 40 bytes of page 5, nothing after.
        assert!(f.contains(r#""page_landed":["#), "{f}");
        let landed: Vec<u32> = f
            .split(r#""page_landed":["#)
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect();
        assert!(landed[4] > 0 && landed[5] > 0, "{landed:?}");
        assert!(landed[6..].iter().all(|&n| n == 0), "{landed:?}");
    }

    #[test]
    fn a_clean_cut_between_pages_is_an_op_boundary_and_one_in_the_journal_is_seen() {
        let mut nor = FakeNor::new();
        let (_, next) = boot(&mut nor);
        nor.cut(CYCLE_OPS + 3 + 4, Tear::Clean);
        work_until_cut(&mut nor, next);
        let (records, _) = boot(&mut nor);
        let f = in_flight(&records);
        assert!(f.contains(r#""shape":"op-boundary""#), "{f}");

        // A cut in the second journal copy's program, half the entry landed:
        // copy 0 names the new cycle, copy 1 shows the torn slot.
        let (_, next) = boot(&mut nor);
        nor.cut(CYCLE_OPS + 1, Tear::Prefix(8));
        work_until_cut(&mut nor, next);
        let (records, next_after) = boot(&mut nor);
        let j1 = records
            .iter()
            .find(|r| r.contains(r#""kind":"ft-journal","copy":1"#))
            .unwrap();
        assert!(j1.contains(r#""shape":"byte-prefix""#), "{j1}");
        assert!(j1.contains(r#""prefix_bytes":8"#), "{j1}");
        // The in-flight cycle is the one copy 0 holds: its sector is old.
        assert!(in_flight(&records).contains(r#""verdict":"old""#));
        assert!(records.iter().any(|r| r.contains(r#""journal_copies":1"#)));
        assert_eq!(next_after, next + 3);
    }
}
