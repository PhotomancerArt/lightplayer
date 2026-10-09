//! How a work cycle programs its sector: the writes, in order.
//!
//! Two plans, one per payload:
//!
//! - **[`ProgramMode::Pages`]** (`flash-tears`): sixteen 256-byte writes, one
//!   per NOR page, every one page-aligned. The 200 cuts of the first sitting
//!   were all this plan, so their torn prefixes could not say whether the
//!   part's (or the ROM's) 32-byte program commands are counted from the
//!   write's address or from absolute 32-byte boundaries: on a page-aligned
//!   write the two are the same offsets.
//! - **[`ProgramMode::Unaligned`]** (`flash-tears-unaligned`, tree-store M2
//!   P10's question): one 20-byte write at offset 0, then writes of
//!   16–1,040 bytes (a multiple of 16) back to back to the end of the sector,
//!   so every write after the first starts at `20 + k·16` — 4 or 20 past a
//!   32-byte boundary, never on one — and most cross a page. The lengths are
//!   drawn from (sector, cycle), so where a write starts inside its page
//!   moves from cycle to cycle. A torn prefix then stops either `32·n` bytes
//!   after its write's start (commands counted from the address), on an
//!   absolute 32-byte boundary, or neither.
//!
//! The whole sector is still programmed, with the same pattern
//! ([`super::pattern`]), so old and new stay complements and the scan's
//! classifier is unchanged; only the cut points inside the program move.
//! `scripts/emu/flash-tears-analyze.py` recomputes the plan and checks it
//! against the writes the in-flight record lists.

use super::pattern::splitmix;
use super::{PAGE_SIZE, PAGES_PER_SECTOR, SECTOR_SIZE};

/// The first write of an unaligned plan: offset 0, this many bytes.
pub const UNALIGNED_FIRST: usize = 20;

/// Every unaligned write's length is a multiple of this.
pub const UNALIGNED_STEP: usize = 16;

/// The longest unaligned write: 65 steps.
pub const UNALIGNED_MAX_WRITE: usize = 65 * UNALIGNED_STEP;

/// The longest write either plan makes: what a harness's bounce buffer holds.
pub const MAX_WRITE: usize = if UNALIGNED_MAX_WRITE > PAGE_SIZE {
    UNALIGNED_MAX_WRITE
} else {
    PAGE_SIZE
};

/// Which plan a work cycle programs its sector with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramMode {
    /// Sixteen page-aligned 256-byte writes.
    Pages,
    /// A 20-byte write, then 16–1,040-byte writes starting at `20 + k·16`.
    Unaligned,
}

impl ProgramMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pages => "pages",
            Self::Unaligned => "unaligned",
        }
    }

    /// The payload that programs this way.
    pub const fn payload(self) -> &'static str {
        match self {
            Self::Pages => super::PAYLOAD,
            Self::Unaligned => super::PAYLOAD_UNALIGNED,
        }
    }
}

/// The writes `cycle` makes into `sector`, as `(offset in the sector,
/// length)`, in order. They tile the sector exactly, and every length is a
/// multiple of 4 (the flash driver's write unit).
pub fn writes(mode: ProgramMode, sector: u32, cycle: u32) -> Writes {
    Writes {
        mode,
        at: 0,
        index: 0,
        state: splitmix(0x0A11_6AED_0000_0000 ^ (u64::from(sector) << 32) ^ u64::from(cycle)),
    }
}

/// The iterator [`writes`] returns.
#[derive(Clone, Debug)]
pub struct Writes {
    mode: ProgramMode,
    at: usize,
    index: usize,
    state: u64,
}

impl Iterator for Writes {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        if self.at >= SECTOR_SIZE {
            return None;
        }
        let len = match self.mode {
            ProgramMode::Pages => {
                debug_assert!(self.index < PAGES_PER_SECTOR);
                PAGE_SIZE
            }
            ProgramMode::Unaligned if self.index == 0 => UNALIGNED_FIRST,
            ProgramMode::Unaligned => {
                self.state = splitmix(self.state);
                let steps = 1 + (self.state % (UNALIGNED_MAX_WRITE / UNALIGNED_STEP) as u64);
                (steps as usize * UNALIGNED_STEP).min(SECTOR_SIZE - self.at)
            }
        };
        let w = (self.at, len);
        self.at += len;
        self.index += 1;
        Some(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_plan_is_sixteen_pages() {
        let w: [(usize, usize); PAGES_PER_SECTOR] =
            core::array::from_fn(|i| (i * PAGE_SIZE, PAGE_SIZE));
        assert!(writes(ProgramMode::Pages, 3, 99).eq(w.iter().copied()));
    }

    #[test]
    fn an_unaligned_plan_tiles_the_sector_off_every_32_byte_boundary() {
        for sector in 0..16 {
            for cycle in [0, 1, 16, 17, 1000, 46_528] {
                let mut next = 0;
                let mut n = 0;
                let mut crosses = 0;
                for (i, (at, len)) in writes(ProgramMode::Unaligned, sector, cycle).enumerate() {
                    assert_eq!(at, next, "contiguous");
                    assert_eq!(len % 4, 0, "a whole number of words");
                    assert!(len > 0 && len <= UNALIGNED_MAX_WRITE);
                    if i == 0 {
                        assert_eq!((at, len), (0, UNALIGNED_FIRST));
                    } else {
                        assert_eq!((at - UNALIGNED_FIRST) % UNALIGNED_STEP, 0);
                        assert_ne!(at % 32, 0, "never on a command boundary");
                    }
                    if at / PAGE_SIZE != (at + len - 1) / PAGE_SIZE {
                        crosses += 1;
                    }
                    next = at + len;
                    n += 1;
                }
                assert_eq!(next, SECTOR_SIZE);
                assert!(n >= 4, "sector {sector} cycle {cycle}: {n} writes");
                assert!(crosses > 0);
            }
        }
    }

    #[test]
    fn the_plan_moves_with_the_cycle() {
        let a: [Option<(usize, usize)>; 4] = {
            let mut w = writes(ProgramMode::Unaligned, 2, 10);
            core::array::from_fn(|_| w.next())
        };
        let b: [Option<(usize, usize)>; 4] = {
            let mut w = writes(ProgramMode::Unaligned, 2, 26);
            core::array::from_fn(|_| w.next())
        };
        assert_ne!(a, b);
        assert!(writes(ProgramMode::Unaligned, 2, 10).eq(writes(ProgramMode::Unaligned, 2, 10)));
    }
}
