//! Classifying what a sector holds after a cut.
//!
//! A sector is read [`super::SCAN_READS`] times. Two accumulators keep what
//! matters: `and` (a bit that read 0 even once is 0 here) and `or` (a bit
//! that read 1 even once is 1 here). Where they agree the bit is **stable**;
//! where they differ it is **weak** — it read differently across the reads,
//! the "weak bit" `lp-nor-sim` models after a torn erase.
//!
//! The in-flight sector's old and new patterns are complements
//! ([`super::pattern`]), so every stable 0 is either an **old** 0 the erase
//! has not reached or a **new** 0 a program put there, never both. That is
//! what lets [`analyze_in_flight`] tell the phases apart without guessing.

use super::{PAGE_SIZE, PAGES_PER_SECTOR, SECTOR_SIZE};

/// Fold one read into the accumulators. On the first read, pass
/// `first = true` and the accumulators are overwritten.
pub fn accumulate(and: &mut [u8], or: &mut [u8], read: &[u8], first: bool) {
    debug_assert!(and.len() == read.len() && or.len() == read.len());
    if first {
        and.copy_from_slice(read);
        or.copy_from_slice(read);
        return;
    }
    for i in 0..read.len() {
        and[i] &= read[i];
        or[i] |= read[i];
    }
}

/// The shape a torn program left on a blank background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TearShape {
    /// Not one intended clear landed.
    Nothing,
    /// Every intended clear landed.
    Complete,
    /// Whole pages landed, then nothing: a cut between two page programs,
    /// not a tear.
    PageBoundary,
    /// A prefix of whole bytes, at most one partial byte, then nothing —
    /// `lp-nor-sim`'s `BytePrefix`.
    BytePrefix,
    /// Clears landed with no such prefix — `lp-nor-sim`'s `RandomBits`, or
    /// something between the two.
    Scattered,
    /// A bit the program did not ask to clear is 0. Neither model makes
    /// this.
    Stray,
}

impl TearShape {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Complete => "complete",
            Self::PageBoundary => "page-boundary",
            Self::BytePrefix => "byte-prefix",
            Self::Scattered => "scattered",
            Self::Stray => "stray",
        }
    }
}

/// What a program that was meant to write `intended` onto erased cells left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramTear {
    pub shape: TearShape,
    /// Bits `intended` asks to clear.
    pub intended_bits: u32,
    /// Of those, cleared.
    pub landed_bits: u32,
    /// Bits cleared that `intended` leaves at 1.
    pub stray_bits: u32,
    /// Leading bytes exactly as intended.
    pub prefix_bytes: u32,
    /// Bytes neither as intended nor `0xFF`.
    pub partial_bytes: u32,
}

/// Classify `actual` against `intended`, both read from erased cells.
pub fn program_tear(actual: &[u8], intended: &[u8]) -> ProgramTear {
    debug_assert_eq!(actual.len(), intended.len());
    let mut t = ProgramTear {
        shape: TearShape::Nothing,
        intended_bits: 0,
        landed_bits: 0,
        stray_bits: 0,
        prefix_bytes: 0,
        partial_bytes: 0,
    };
    let mut in_prefix = true;
    for (&a, &w) in actual.iter().zip(intended) {
        t.intended_bits += (!w).count_ones();
        t.landed_bits += (!a & !w).count_ones();
        t.stray_bits += (!a & w).count_ones();
        if in_prefix && a == w {
            t.prefix_bytes += 1;
        } else {
            in_prefix = false;
        }
        if a != w && a != 0xFF {
            t.partial_bytes += 1;
        }
    }
    let p = t.prefix_bytes as usize;
    t.shape = if t.stray_bits > 0 {
        TearShape::Stray
    } else if t.landed_bits == t.intended_bits {
        TearShape::Complete
    } else if t.landed_bits == 0 {
        TearShape::Nothing
    } else if whole_pages(actual, intended) {
        TearShape::PageBoundary
    } else if actual.get(p + 1..).is_none_or(|rest| rest.iter().all(|&b| b == 0xFF)) {
        TearShape::BytePrefix
    } else {
        TearShape::Scattered
    };
    t
}

/// Is every page of `actual` either exactly `intended` or blank? Only a
/// span of whole pages can be; a shorter one (a journal entry) never is.
fn whole_pages(actual: &[u8], intended: &[u8]) -> bool {
    actual.len() % PAGE_SIZE == 0
        && actual
            .chunks(PAGE_SIZE)
            .zip(intended.chunks(PAGE_SIZE))
            .all(|(a, w)| a == w || a.iter().all(|&b| b == 0xFF))
}

/// What the in-flight sector looked like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The old pattern, untouched: the cut came before the erase changed a
    /// cell.
    Old,
    /// All `0xFF`, no weak bits: erased, nothing programmed yet.
    Erased,
    /// Every stable bit 1, but some bits are weak: reads erased and is not.
    ErasedWeak,
    /// The new pattern, whole: the cut came after the program.
    Complete,
    /// Some old zeros left, no new zeros: a torn erase.
    TornErase,
    /// No old zeros left, some new zeros: a torn (or interrupted) program.
    TornProgram,
    /// Old zeros AND new zeros at once: neither phase's shape.
    Mixed,
}

impl Verdict {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::Erased => "erased",
            Self::ErasedWeak => "erased-weak",
            Self::Complete => "complete",
            Self::TornErase => "torn-erase",
            Self::TornProgram => "torn-program",
            Self::Mixed => "mixed",
        }
    }
}

/// The in-flight sector, measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InFlight {
    pub verdict: Verdict,
    /// Bits that read differently across the reads.
    pub weak_bits: u32,
    /// Bytes with at least one weak bit.
    pub weak_bytes: u32,
    /// Bits the old pattern had at 0.
    pub old_zero_bits: u32,
    /// Of those, still 0 on every read (not erased).
    pub remaining_old_zeros: u32,
    /// Of those, weak.
    pub weak_at_old_zeros: u32,
    /// Bits the new pattern wants at 0.
    pub new_zero_bits: u32,
    /// Of those, 0 on every read (programmed).
    pub landed_new_zeros: u32,
    /// Of those, weak.
    pub weak_at_new_zeros: u32,
    /// Bytes that read `0xFF` on every read.
    pub ff_bytes: u32,
    /// Bytes from offset 0 that read `0xFF` on every read.
    pub leading_ff_bytes: u32,
    /// Bytes back from the end that read `0xFF` on every read.
    pub trailing_ff_bytes: u32,
    /// Per program page: old zeros remaining.
    pub page_remaining: [u16; PAGES_PER_SECTOR],
    /// Per program page: new zeros landed.
    pub page_landed: [u16; PAGES_PER_SECTOR],
    /// Per program page: weak bits.
    pub page_weak: [u16; PAGES_PER_SECTOR],
    /// The program's shape, judged with weak bits read as 1 (not landed).
    /// Only for [`Verdict::TornProgram`].
    pub program: Option<ProgramTear>,
}

/// Classify the in-flight sector. `old` and `new` must be complements.
pub fn analyze_in_flight(and: &[u8], or: &[u8], old: &[u8], new: &[u8]) -> InFlight {
    debug_assert!(and.len() == SECTOR_SIZE && or.len() == SECTOR_SIZE);
    debug_assert!(old.iter().zip(new).all(|(o, n)| *o == !*n));
    let mut f = InFlight {
        verdict: Verdict::Old,
        weak_bits: 0,
        weak_bytes: 0,
        old_zero_bits: 0,
        remaining_old_zeros: 0,
        weak_at_old_zeros: 0,
        new_zero_bits: 0,
        landed_new_zeros: 0,
        weak_at_new_zeros: 0,
        ff_bytes: 0,
        leading_ff_bytes: 0,
        trailing_ff_bytes: 0,
        page_remaining: [0; PAGES_PER_SECTOR],
        page_landed: [0; PAGES_PER_SECTOR],
        page_weak: [0; PAGES_PER_SECTOR],
        program: None,
    };
    let mut leading = true;
    for i in 0..SECTOR_SIZE {
        let weak = and[i] ^ or[i];
        // `or` is 0 only where every read was 0: a stable zero.
        let stable_zero = !or[i];
        let old_zero = !old[i];
        let new_zero = !new[i];
        let page = i / PAGE_SIZE;
        f.weak_bits += weak.count_ones();
        f.weak_bytes += u32::from(weak != 0);
        f.old_zero_bits += old_zero.count_ones();
        f.new_zero_bits += new_zero.count_ones();
        let remaining = (stable_zero & old_zero).count_ones();
        let landed = (stable_zero & new_zero).count_ones();
        f.remaining_old_zeros += remaining;
        f.landed_new_zeros += landed;
        f.weak_at_old_zeros += (weak & old_zero).count_ones();
        f.weak_at_new_zeros += (weak & new_zero).count_ones();
        f.page_remaining[page] += remaining as u16;
        f.page_landed[page] += landed as u16;
        f.page_weak[page] += weak.count_ones() as u16;
        let ff = weak == 0 && and[i] == 0xFF;
        f.ff_bytes += u32::from(ff);
        if leading && ff {
            f.leading_ff_bytes += 1;
        } else {
            leading = false;
        }
    }
    for i in (0..SECTOR_SIZE).rev() {
        if and[i] == 0xFF && or[i] == 0xFF {
            f.trailing_ff_bytes += 1;
        } else {
            break;
        }
    }
    let (r, l, w) = (f.remaining_old_zeros, f.landed_new_zeros, f.weak_bits);
    f.verdict = if w == 0 && l == 0 && r == f.old_zero_bits {
        Verdict::Old
    } else if w == 0 && r == 0 && l == f.new_zero_bits {
        Verdict::Complete
    } else if r == 0 && l == 0 {
        if w == 0 {
            Verdict::Erased
        } else {
            Verdict::ErasedWeak
        }
    } else if l == 0 {
        Verdict::TornErase
    } else if r == 0 {
        Verdict::TornProgram
    } else {
        Verdict::Mixed
    };
    if f.verdict == Verdict::TornProgram {
        f.program = Some(program_tear(or, new));
    }
    f
}

/// A sector that was not in flight, measured against what it should hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settled {
    /// Stable, and exactly as expected.
    pub complete: bool,
    /// Stable bits that differ from what was expected.
    pub diff_bits: u32,
    pub weak_bits: u32,
}

/// Compare a settled sector with what its last cycle wrote.
pub fn analyze_settled(and: &[u8], or: &[u8], expected: &[u8]) -> Settled {
    let mut s = Settled {
        complete: false,
        diff_bits: 0,
        weak_bits: 0,
    };
    for i in 0..expected.len() {
        let weak = and[i] ^ or[i];
        s.weak_bits += weak.count_ones();
        s.diff_bits += ((and[i] ^ expected[i]) & !weak).count_ones();
    }
    s.complete = s.weak_bits == 0 && s.diff_bits == 0;
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::flash_tears::pattern::fill_pattern;

    fn pats() -> ([u8; SECTOR_SIZE], [u8; SECTOR_SIZE]) {
        let mut old = [0u8; SECTOR_SIZE];
        let mut new = [0u8; SECTOR_SIZE];
        fill_pattern(2, 2, &mut old);
        fill_pattern(2, 18, &mut new);
        (old, new)
    }

    fn one_read(v: &[u8]) -> InFlight {
        let (old, new) = pats();
        analyze_in_flight(v, v, &old, &new)
    }

    #[test]
    fn the_four_clean_states_are_named() {
        let (old, new) = pats();
        assert_eq!(one_read(&old).verdict, Verdict::Old);
        assert_eq!(one_read(&new).verdict, Verdict::Complete);
        assert_eq!(one_read(&[0xFF; SECTOR_SIZE]).verdict, Verdict::Erased);
        // Two whole pages of the new pattern, then nothing: an interrupted
        // program that is not a tear.
        let mut v = [0xFFu8; SECTOR_SIZE];
        v[..512].copy_from_slice(&new[..512]);
        let f = one_read(&v);
        assert_eq!(f.verdict, Verdict::TornProgram);
        assert_eq!(f.program.unwrap().shape, TearShape::PageBoundary);
        assert!(f.program.unwrap().prefix_bytes >= 512);
        assert_eq!(f.page_landed[2], 0);
    }

    #[test]
    fn a_byte_prefix_and_a_scatter_are_told_apart() {
        let (_, new) = pats();
        // A byte with at least two clears to make, so landing one of them
        // leaves it partial.
        let at = (300..SECTOR_SIZE)
            .find(|&i| (!new[i]).count_ones() >= 2)
            .unwrap();
        let mut v = [0xFFu8; SECTOR_SIZE];
        v[..at].copy_from_slice(&new[..at]);
        let clears = !new[at];
        v[at] = !(clears & clears.wrapping_neg());
        let p = one_read(&v).program.unwrap();
        assert_eq!(p.shape, TearShape::BytePrefix);
        assert_eq!(p.prefix_bytes, at as u32);
        assert_eq!(p.partial_bytes, 1);

        let mut v = [0xFFu8; SECTOR_SIZE];
        for i in (256..512).step_by(3) {
            v[i] = new[i];
        }
        let f = one_read(&v);
        assert_eq!(f.verdict, Verdict::TornProgram);
        assert_eq!(f.program.unwrap().shape, TearShape::Scattered);
        assert_eq!(f.page_landed[0], 0);
        assert!(f.page_landed[1] > 0);
    }

    #[test]
    fn a_torn_erase_counts_what_is_left_and_where() {
        let (old, new) = pats();
        // The first half erased, the second half old.
        let mut v = old;
        v[..2048].fill(0xFF);
        let f = one_read(&v);
        assert_eq!(f.verdict, Verdict::TornErase);
        assert_eq!(f.leading_ff_bytes, 2048);
        assert_eq!(f.page_remaining[0], 0);
        assert!(f.page_remaining[15] > 0);
        // An old zero and a new zero at once is neither phase.
        let mut v = old;
        let i = (0..SECTOR_SIZE).find(|&i| new[i] != 0xFF).unwrap();
        v[i] &= new[i];
        assert_eq!(one_read(&v).verdict, Verdict::Mixed);
    }

    #[test]
    fn weak_bits_are_the_reads_that_disagree() {
        let (old, new) = pats();
        let ff = [0xFFu8; SECTOR_SIZE];
        let mut flicker = ff;
        flicker[100] = 0xFE;
        let (mut and, mut or) = ([0u8; SECTOR_SIZE], [0u8; SECTOR_SIZE]);
        accumulate(&mut and, &mut or, &ff, true);
        accumulate(&mut and, &mut or, &flicker, false);
        let f = analyze_in_flight(&and, &or, &old, &new);
        assert_eq!(f.weak_bits, 1);
        assert_eq!(f.verdict, Verdict::ErasedWeak);
        assert_eq!(f.leading_ff_bytes, 100);
        let s = analyze_settled(&and, &or, &ff);
        assert_eq!((s.complete, s.diff_bits, s.weak_bits), (false, 0, 1));
        assert!(analyze_settled(&new, &new, &new).complete);
    }

    #[test]
    fn a_stray_clear_is_neither_model() {
        let intended = [0xF0u8; 16];
        let mut actual = [0xFFu8; 16];
        actual[0] = 0xF0;
        actual[1] = 0x7F;
        let t = program_tear(&actual, &intended);
        assert_eq!(t.shape, TearShape::Stray);
        assert_eq!(t.stray_bits, 1);
    }
}
