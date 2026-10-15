//! The payload's `[fw-check-json] ` records, as hand-written JSON (no
//! `alloc`, one record a line). `scripts/emu/flash-reprogram-analyze.py`
//! reads these exact field names.

use core::fmt;

use super::{CONTROL_CHUNKS, MAX_KEEP};

/// `fr-boot`: one per boot, first.
pub struct BootRecord<'a> {
    pub reset: &'a str,
    pub mac: [u8; 6],
    pub flash_id: u32,
    pub base: u32,
    pub sectors: u32,
    /// `fresh` (no state: the region is set up) or `resume`.
    pub fresh: bool,
    pub seq: u32,
    /// The generation the loop before this boot programmed with.
    pub generation: u32,
    pub keep: [Option<u32>; MAX_KEEP],
    pub pending: usize,
    /// What the loop before this boot did (`program`, `erase`; `none` on a
    /// fresh region).
    pub loop_mode: &'a str,
}

impl fmt::Display for BootRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.mac;
        write!(
            f,
            "{{\"kind\":\"fr-boot\",\"reset\":\"{}\",\"mac\":\"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\",\"flash_id\":\"0x{:06x}\",\"base\":\"0x{:x}\",\"sectors\":{},\"state\":\"{}\",\"seq\":{},\"generation\":{},\"keep\":{},\"pending\":{},\"loop\":\"{}\"}}",
            self.reset,
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
            self.flash_id,
            self.base,
            self.sectors,
            if self.fresh { "fresh" } else { "resume" },
            self.seq,
            self.generation,
            List(&self.keep),
            self.pending,
            self.loop_mode,
        )
    }
}

/// `fr-scan`: the region read once, in program order.
pub struct ScanRecord {
    pub generation: u32,
    /// Bytes the loop programs (the region less its kept sectors).
    pub order_bytes: u32,
    /// Logical byte offset of the first word that is not as meant; `None`:
    /// the loop filled the region before the cut.
    pub frontier: Option<u32>,
    pub frontier_addr: Option<u32>,
    /// Logical byte offset of the last word that is not erased.
    pub last_dirty: Option<u32>,
    /// Words from the frontier through the last dirty word, by what they
    /// read: as meant, erased, or neither.
    pub span_words: u32,
    pub span_eq: u32,
    pub span_ff: u32,
    pub span_other: u32,
    pub scan_us: u64,
}

impl fmt::Display for ScanRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-scan\",\"generation\":{},\"order_bytes\":{},\"frontier\":{},\"frontier_addr\":{},\"last_dirty\":{},\"span_words\":{},\"span_eq\":{},\"span_ff\":{},\"span_other\":{},\"scan_us\":{}}}",
            self.generation,
            self.order_bytes,
            Opt(self.frontier),
            OptHex(self.frontier_addr),
            Opt(self.last_dirty),
            self.span_words,
            self.span_eq,
            self.span_ff,
            self.span_other,
            self.scan_us,
        )
    }
}

/// `fr-window`: the bytes around the frontier, read `reads` times.
pub struct WindowRecord {
    pub start: u32,
    pub end: u32,
    pub reads: usize,
    /// The frontier was further from the last dirty word than the window
    /// holds (never seen on a clean tear).
    pub capped: bool,
    pub weak_words: u32,
    pub weak_bits: u32,
    pub torn_words: u32,
    /// Torn words followed through control, re-program and re-check.
    pub followed: usize,
}

impl fmt::Display for WindowRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-window\",\"start\":{},\"end\":{},\"reads\":{},\"capped\":{},\"weak_words\":{},\"weak_bits\":{},\"torn_words\":{},\"followed\":{}}}",
            self.start,
            self.end,
            self.reads,
            self.capped,
            self.weak_words,
            self.weak_bits,
            self.torn_words,
            self.followed,
        )
    }
}

/// `fr-torn`: one torn word as the window's reads found it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TornWord {
    pub addr: u32,
    /// Logical byte offset.
    pub at: u32,
    pub intended: u32,
    /// Bits that read 0 on every read (`and` of the reads is the bits that
    /// read 1 on every read).
    pub and: u32,
    pub or: u32,
}

impl TornWord {
    /// Bits that read both ways.
    pub fn weak(&self) -> u32 {
        self.and ^ self.or
    }
    /// Bits meant to be 0.
    pub fn clears(&self) -> u32 {
        !self.intended
    }
    /// Meant 0, read 0 every time.
    pub fn landed(&self) -> u32 {
        self.clears() & !self.or
    }
    /// Meant 0, read 1 every time.
    pub fn unlanded(&self) -> u32 {
        self.clears() & self.and
    }
    /// Meant 1, read 0 every time.
    pub fn wrong(&self) -> u32 {
        self.intended & !self.or
    }
}

impl fmt::Display for TornWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let weak = self.weak();
        write!(
            f,
            "{{\"kind\":\"fr-torn\",\"addr\":\"0x{:x}\",\"at\":{},\"in_write\":{},\"in_command\":{},\"intended\":\"0x{:08x}\",\"and\":\"0x{:08x}\",\"or\":\"0x{:08x}\",\"clears\":{},\"landed\":{},\"unlanded\":{},\"weak\":{},\"weak_clears\":{},\"weak_ones\":{},\"wrong\":{}}}",
            self.addr,
            self.at,
            self.addr % super::WRITE_SIZE as u32,
            self.addr % super::COMMAND_SIZE as u32,
            self.intended,
            self.and,
            self.or,
            self.clears().count_ones(),
            self.landed().count_ones(),
            self.unlanded().count_ones(),
            weak.count_ones(),
            (weak & self.clears()).count_ones(),
            (weak & self.intended).count_ones(),
            self.wrong().count_ones(),
        )
    }
}

/// Per-bit zero counts of one word over [`CONTROL_CHUNKS`] chunks of reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordReads {
    pub addr: u32,
    pub intended: u32,
    pub chunk_reads: u32,
    /// `zeros[c][b]`: reads in chunk `c` on which bit `b` read 0.
    pub zeros: [[u16; 32]; CONTROL_CHUNKS],
}

impl WordReads {
    pub fn new(addr: u32, intended: u32, chunk_reads: u32) -> Self {
        Self {
            addr,
            intended,
            chunk_reads,
            zeros: [[0; 32]; CONTROL_CHUNKS],
        }
    }

    pub fn count(&mut self, chunk: usize, word: u32) {
        for (b, z) in self.zeros[chunk].iter_mut().enumerate() {
            if word & (1 << b) == 0 {
                *z += 1;
            }
        }
    }

    fn total(&self, b: usize) -> u32 {
        self.zeros.iter().map(|c| u32::from(c[b])).sum()
    }

    /// Bits that read 1 on every read.
    pub fn and(&self) -> u32 {
        (0..32)
            .filter(|&b| self.total(b) == 0)
            .fold(0, |m, b| m | 1 << b)
    }

    /// Bits that read 1 on some read.
    pub fn or(&self) -> u32 {
        let all = self.chunk_reads * CONTROL_CHUNKS as u32;
        (0..32)
            .filter(|&b| self.total(b) < all)
            .fold(0, |m, b| m | 1 << b)
    }

    pub fn stable(&self) -> bool {
        self.and() == self.or()
    }

    pub fn correct(&self) -> bool {
        self.stable() && self.and() == self.intended
    }
}

/// `fr-reads`: one torn word read [`super::CONTROL_READS`] times at one
/// stage — `before` the re-program (the control), `after` it, or at the
/// next boot's `recheck`.
pub struct ReadsRecord<'a> {
    pub stage: &'a str,
    pub reads: &'a WordReads,
    pub gap_us: u64,
}

impl fmt::Display for ReadsRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = self.reads;
        let (and, or) = (r.and(), r.or());
        write!(
            f,
            "{{\"kind\":\"fr-reads\",\"stage\":\"{}\",\"addr\":\"0x{:x}\",\"intended\":\"0x{:08x}\",\"reads\":{},\"chunks\":{},\"gap_us\":{},\"and\":\"0x{:08x}\",\"or\":\"0x{:08x}\",\"stable\":{},\"correct\":{},\"bits\":[",
            self.stage,
            r.addr,
            r.intended,
            r.chunk_reads * CONTROL_CHUNKS as u32,
            CONTROL_CHUNKS,
            self.gap_us,
            and,
            or,
            r.stable(),
            r.correct(),
        )?;
        let mut first = true;
        for b in (0..32).filter(|&b| (and ^ or) & (1 << b) != 0) {
            if !first {
                f.write_str(",")?;
            }
            first = false;
            write!(f, "{{\"bit\":{b},\"zeros\":[")?;
            for (c, chunk) in r.zeros.iter().enumerate() {
                if c > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{}", chunk[b])?;
            }
            f.write_str("]}")?;
        }
        f.write_str("]}")
    }
}

/// `fr-write`: one re-programmed write's bytes, read `reads` times, `after`
/// the re-program or at the next boot's `recheck`.
pub struct WriteRecord<'a> {
    pub stage: &'a str,
    /// The write's first byte.
    pub write: u32,
    /// Bytes re-programmed from it.
    pub span: u32,
    pub reads: usize,
    /// Every re-programmed word read as meant on every read.
    pub span_ok: bool,
    /// Every word after the span (to the write's end) read erased on every
    /// read: the re-program disturbed nothing it was not asked to.
    pub tail_ok: bool,
    pub weak_words: u32,
    pub wrong_words: u32,
}

impl fmt::Display for WriteRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-write\",\"stage\":\"{}\",\"write\":\"0x{:x}\",\"span\":{},\"reads\":{},\"span_ok\":{},\"tail_ok\":{},\"weak_words\":{},\"wrong_words\":{}}}",
            self.stage,
            self.write,
            self.span,
            self.reads,
            self.span_ok,
            self.tail_ok,
            self.weak_words,
            self.wrong_words,
        )
    }
}

/// `fr-reprogram`: one program call over a torn word's write.
pub struct ReprogramRecord {
    pub write: u32,
    pub span: u32,
    pub us: u64,
}

impl fmt::Display for ReprogramRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-reprogram\",\"write\":\"0x{:x}\",\"span\":{},\"us\":{}}}",
            self.write, self.span, self.us,
        )
    }
}

/// `fr-prepare`: the region made ready for the next loop.
pub struct PrepareRecord {
    pub erased_sectors: u32,
    pub erase_us: u64,
    /// Every erased sector read back all `0xFF`.
    pub blank: bool,
    pub keep: [Option<u32>; MAX_KEEP],
    pub pending: usize,
    pub next_generation: u32,
    /// Sectors programmed with the next generation's pattern for an erase
    /// loop to erase (0 before a program loop).
    pub programmed_sectors: u32,
}

impl fmt::Display for PrepareRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-prepare\",\"erased_sectors\":{},\"erase_us\":{},\"blank\":{},\"keep\":{},\"pending\":{},\"next_generation\":{},\"next_loop\":\"{}\",\"programmed_sectors\":{}}}",
            self.erased_sectors,
            self.erase_us,
            self.blank,
            List(&self.keep),
            self.pending,
            self.next_generation,
            super::mode_of(self.next_generation).name(),
            self.programmed_sectors,
        )
    }
}

/// `fr-escan`: an erase loop's sectors, read once each, in order.
pub struct EraseScanRecord {
    pub generation: u32,
    pub sectors: u32,
    /// The loop's index of the first sector that does not read all `0xFF`;
    /// `None`: the loop erased them all before the cut.
    pub first_dirty: Option<u32>,
    /// That sector: `pattern` (the erase never touched it) or `other`.
    pub first_class: &'static str,
    pub scan_us: u64,
}

impl fmt::Display for EraseScanRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-escan\",\"generation\":{},\"sectors\":{},\"first_dirty\":{},\"first_class\":\"{}\",\"scan_us\":{}}}",
            self.generation,
            self.sectors,
            Opt(self.first_dirty),
            self.first_class,
            self.scan_us,
        )
    }
}

/// `fr-esector`: one torn-erase sector read `reads` times at one stage —
/// `window` (64 reads, as found), `after` it was programmed to `0x00`, or
/// the next boot's `recheck`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EraseSectorRecord {
    pub stage: &'static str,
    pub addr: u32,
    /// The loop's index of the sector.
    pub index: u32,
    pub reads: u32,
    /// Bits that read both ways.
    pub weak: u32,
    /// Bits that read 0 on every read.
    pub stable_zero: u32,
    /// Bits that read 1 on every read.
    pub stable_one: u32,
    /// Bytes from the sector's start that read `0x00` on every read.
    pub leading_zero_bytes: u32,
    /// Bits whose stable value is the old pattern's (`window` only: an
    /// erase that had not lifted them yet, or never pre-programmed them).
    pub as_pattern: u32,
}

impl EraseSectorRecord {
    /// Every bit reads 0 on every read.
    pub fn zeroed(&self) -> bool {
        self.weak == 0 && self.stable_one == 0
    }
}

impl fmt::Display for EraseSectorRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-esector\",\"stage\":\"{}\",\"addr\":\"0x{:x}\",\"index\":{},\"reads\":{},\"weak\":{},\"stable_zero\":{},\"stable_one\":{},\"leading_zero_bytes\":{},\"as_pattern\":{},\"zeroed\":{}}}",
            self.stage,
            self.addr,
            self.index,
            self.reads,
            self.weak,
            self.stable_zero,
            self.stable_one,
            self.leading_zero_bytes,
            self.as_pattern,
            self.zeroed(),
        )
    }
}

/// `fr-econtrol`: a torn-erase sector read [`super::SECTOR_READS`] more
/// times, left alone, against its 64 window reads.
pub struct EraseControlRecord {
    pub addr: u32,
    pub reads: u32,
    /// Weak in the window's reads.
    pub weak_window: u32,
    /// Weak in the control's reads.
    pub weak_control: u32,
    /// Weak in both.
    pub weak_both: u32,
    /// Weak in the window, stable through every control read.
    pub settled_alone: u32,
    /// Stable in the window, weak in the control.
    pub new_weak: u32,
    /// Bits stable in both but read differently (none expected).
    pub moved: u32,
}

impl fmt::Display for EraseControlRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-econtrol\",\"addr\":\"0x{:x}\",\"reads\":{},\"gap_us\":{},\"weak_window\":{},\"weak_control\":{},\"weak_both\":{},\"settled_alone\":{},\"new_weak\":{},\"moved\":{}}}",
            self.addr,
            self.reads,
            super::CONTROL_GAP_US,
            self.weak_window,
            self.weak_control,
            self.weak_both,
            self.settled_alone,
            self.new_weak,
            self.moved,
        )
    }
}

/// `fr-timing`: one sector erase and [`super::TIMING_WRITES`] writes in the
/// scratch sector, timed.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimingRecord {
    pub erase_us: u64,
    pub write_us_min: u64,
    pub write_us_max: u64,
    pub write_us_total: u64,
}

impl fmt::Display for TimingRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"fr-timing\",\"erase_us\":{},\"writes\":{},\"write_us_min\":{},\"write_us_max\":{},\"write_us_total\":{}}}",
            self.erase_us,
            super::TIMING_WRITES,
            self.write_us_min,
            self.write_us_max,
            self.write_us_total,
        )
    }
}

struct Opt(Option<u32>);

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        }
    }
}

struct OptHex(Option<u32>);

impl fmt::Display for OptHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(v) => write!(f, "\"0x{v:x}\""),
            None => f.write_str("null"),
        }
    }
}

struct List<'a>(&'a [Option<u32>]);

impl fmt::Display for List<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, v) in self.0.iter().flatten().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{v}")?;
        }
        f.write_str("]")
    }
}
