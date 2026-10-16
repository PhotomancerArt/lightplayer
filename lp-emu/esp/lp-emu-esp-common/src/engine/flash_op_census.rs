//! The in-range flash op census: which program and erase commands a run
//! sent into one address range, in order.
//!
//! What a cut sweep needs before it cuts (plan D6): a dry run counts the
//! commands a scenario sends into the store's partition, and the trace says
//! which index is which kind at which address, so a sampler can aim at the
//! erases or at a root write. It counts exactly what a [`FlashCut`] counts —
//! page programs and sector/block erases the part executed, never reads,
//! WREN or status polls (plan Q7) — so an index from the census is an index
//! a cut can be aimed at, when both start counting at the same moment.
//!
//! Off by default and cheap when off: the chip holds `None`, and a command
//! outside the range costs one comparison.
//!
//! [`FlashCut`]: super::flash_cut::FlashCut

use alloc::vec::Vec;
use core::ops::Range;

use super::flash_cut::FlashOpKind;

/// One counted command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashOpRecord {
    /// 0-based, in the order the part executed them, since the census began.
    pub index: u64,
    pub kind: FlashOpKind,
    /// The command's address (an erase's granule base).
    pub addr: u32,
    /// Bytes programmed, or the erase granule.
    pub len: u32,
}

/// The counters, and the trace when one was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashOpCensus {
    /// Flash byte addresses a command must touch to be counted.
    pub range: Range<u32>,
    pub programs: u64,
    /// Bytes the counted programs carried.
    pub program_bytes: u64,
    pub sector_erases: u64,
    pub block_erases: u64,
    trace: Option<Vec<FlashOpRecord>>,
}

impl FlashOpCensus {
    /// Count the commands touching `range`; keep each one's record when
    /// `trace` is set.
    pub fn new(range: Range<u32>, trace: bool) -> Self {
        Self {
            range,
            programs: 0,
            program_bytes: 0,
            sector_erases: 0,
            block_erases: 0,
            trace: trace.then(Vec::new),
        }
    }

    /// Every counted command: programs plus erases.
    pub fn ops(&self) -> u64 {
        self.programs + self.sector_erases + self.block_erases
    }

    /// The trace, when the census keeps one.
    pub fn trace(&self) -> Option<&[FlashOpRecord]> {
        self.trace.as_deref()
    }

    /// A command the part is about to execute. Counted when it touches the
    /// range.
    pub(crate) fn note(&mut self, kind: FlashOpKind, addr: u32, len: u32) {
        if !overlaps(&self.range, addr, len) {
            return;
        }
        let index = self.ops();
        match kind {
            FlashOpKind::Program => {
                self.programs += 1;
                self.program_bytes += u64::from(len);
            }
            FlashOpKind::SectorErase => self.sector_erases += 1,
            FlashOpKind::BlockErase => self.block_erases += 1,
        }
        if let Some(trace) = self.trace.as_mut() {
            trace.push(FlashOpRecord {
                index,
                kind,
                addr,
                len,
            });
        }
    }
}

impl core::fmt::Display for FlashOpCensus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "FLASH-OPS range={:#x}+{:#x} ops={} programs={} program_bytes={} sector_erases={} \
             block_erases={}",
            self.range.start,
            self.range.end - self.range.start,
            self.ops(),
            self.programs,
            self.program_bytes,
            self.sector_erases,
            self.block_erases
        )
    }
}

/// Does `addr..addr + len` share a byte with `range`? A zero-length command
/// touches nothing.
pub(crate) fn overlaps(range: &Range<u32>, addr: u32, len: u32) -> bool {
    let end = addr.saturating_add(len);
    len > 0 && addr < range.end && end > range.start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commands_touching_the_range_are_counted_and_traced() {
        let mut census = FlashOpCensus::new(0x1000..0x3000, true);
        census.note(FlashOpKind::Program, 0x0fc0, 64); // ends at the range
        census.note(FlashOpKind::Program, 0x0ff0, 32); // straddles it in
        census.note(FlashOpKind::SectorErase, 0x2000, 4096);
        census.note(FlashOpKind::SectorErase, 0x3000, 4096); // past it
        census.note(FlashOpKind::BlockErase, 0x0000, 0x1_0000); // covers it
        census.note(FlashOpKind::Program, 0x2000, 0); // empty
        assert_eq!(
            (census.programs, census.sector_erases, census.block_erases),
            (1, 1, 1)
        );
        assert_eq!(census.program_bytes, 32);
        let trace = census.trace().expect("a trace was asked for");
        assert_eq!(
            trace.iter().map(|r| (r.index, r.addr)).collect::<Vec<_>>(),
            vec![(0, 0x0ff0), (1, 0x2000), (2, 0x0000)]
        );
        assert!(
            census
                .to_string()
                .starts_with("FLASH-OPS range=0x1000+0x2000 ops=3"),
            "{census}"
        );
    }

    #[test]
    fn without_a_trace_only_the_counters_move() {
        let mut census = FlashOpCensus::new(0..0x1000, false);
        census.note(FlashOpKind::Program, 0, 16);
        assert_eq!(census.ops(), 1);
        assert!(census.trace().is_none());
    }
}
