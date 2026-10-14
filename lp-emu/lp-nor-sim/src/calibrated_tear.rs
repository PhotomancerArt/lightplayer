//! The `calibrated` tear model: torn erases and programs shaped the way a
//! real part tore, in the proportions it tore them.
//!
//! **Source:** `docs/reports/2026-10-08-c6-nor-tear-calibration.md` — 500
//! power cuts on CX1 (`c6-expendable`, MAC `14:C1:9F:E6:54:90`, a generic
//! ESP32-C6 dev board), flash part JEDEC id `0x464016` (4 MiB), VBUS cut at
//! the board's USB hub, `silicon:esp32c6`, the page-aligned `flash-tears`
//! payload. **It is fitted on those 500 cuts** (it was fitted on the first
//! 200 and re-checked at 500, which confirmed every share it had; the 500
//! showed two things it could not draw, now below): the weights and the
//! tables are counts out of that report's generated section, and
//! `scripts/emu/flash-tears-analyze.py --model-table` prints them again from
//! every committed transcript.
//!
//! What the part did, and what this model therefore does:
//!
//! - **An erase pre-programs the sector to `0x00`, front to back a 4-byte
//!   word at a time, then lifts every cell at once.** A cut lands in one of
//!   five states ([`EraseShape`]): *zeroing* (a word-aligned `0x00` run from
//!   offset 0, the old data whole after it), *all zero*, *erasing* (zeros
//!   left at random positions over the whole sector, old and new positions
//!   alike, with weak bits), *reads `0xFF` with a few weak bits*, and
//!   *reads `0xFF`* — silently: no weak bit, nothing a re-read can catch, but
//!   the erase was never finished. No erase that had started left the old
//!   data whole, so this model never tears one that way.
//! - **A cut can land just before an erase starts** (`cut_before_erase`): in
//!   the gap between the previous operation and the erase command, the
//!   sector left exactly as it was. That is where the cut landed, not a
//!   shape of the erase — the sector is not even tainted. One of 418 cuts at
//!   an erase did (the journal named the cycle, the sector read its old data
//!   whole). A program has the same gap, already inside its weights: a
//!   prefix ending at offset 0 is a cut before the program moved a cell.
//! - **A program stops on the mask ROM's 32-byte command, or inside one on a
//!   4-byte word** ([`ProgramShape`]): a prefix of exactly what was asked,
//!   then nothing. **Or, rarely, inside a word** ([`ProgramShape::MidWord`],
//!   2 of 79): whole words, then one word with only some of its clears
//!   landed and a weak bit, then nothing ([`CX1_MID_WORD`]). The command and
//!   word are counted from the start of the in-flight page operation (every
//!   silicon program was page-aligned, so absolute and relative alignment
//!   were not told apart).
//!
//! Where a cut lands — erase or program — is the workload's business (the
//! op counter), not the model's: only the shape *within* a torn operation
//! (and, at an erase, whether it had started) is drawn here. The silicon cut
//! was uniform in time and 84 % of cuts landed in an erase; a sweep cuts at
//! every op instead.

use alloc::vec::Vec;

use crate::SimRng;

/// One erase command's pre-program unit, and a program's: a 32-bit word.
pub const TEAR_WORD: usize = 4;

/// One program command on the C6's flash path: the mask ROM sends a 256-byte
/// page as eight 32-byte commands.
pub const TEAR_COMMAND: usize = 32;

/// How a torn erase leaves its sector, in the order the part passes through
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EraseShape {
    /// A run of `0x00` from offset 0 ending on a 4-byte word, the old data
    /// whole after it.
    Zeroing,
    /// Every bit 0, stable.
    AllZero,
    /// Lifting from all zero: a residue of stable zeros at random positions
    /// over the whole sector, and weak bits, drawn together from
    /// [`CX1_ERASING`].
    Erasing,
    /// Reads all `0xFF` but carries a few weak bits ([`CX1_READS_FF_WEAK`]).
    ReadsFfWeak,
    /// Reads all `0xFF`, no weak bit: indistinguishable from a finished
    /// erase, but the erase was cut.
    ReadsFf,
}

/// Where a torn program's landed prefix stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProgramShape {
    /// On a 32-byte command boundary (offset 0 included: nothing landed).
    CommandBoundary,
    /// On a 4-byte word inside a command.
    MidCommand,
    /// Inside a 4-byte word: whole words, then one word with some of its
    /// clears landed and weak bits ([`CX1_MID_WORD`]), then nothing.
    MidWord,
}

/// The weights the calibrated model draws its shapes with: counts, not
/// fractions, so a mix is exactly what was observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TearMix {
    pub erase_zeroing: u32,
    pub erase_all_zero: u32,
    pub erase_erasing: u32,
    pub erase_reads_ff_weak: u32,
    pub erase_reads_ff: u32,
    /// Not a shape: cuts at an erase that landed before it started, drawn
    /// against the five erase shapes' total. The sector is left untouched.
    pub cut_before_erase: u32,
    pub program_command_boundary: u32,
    pub program_mid_command: u32,
    pub program_mid_word: u32,
}

impl TearMix {
    /// CX1's 500 cuts: 418 at an erase (17 zeroing, 63 all zero, 69
    /// erasing, 4 reading `0xFF` with weak bits, 264 reading `0xFF`, and 1
    /// before the erase started) and 79 torn programs (58 on a command
    /// boundary, 19 inside a command on a word, 2 inside a word). The three
    /// cuts in the journal's own wrap erase are not cuts of a region
    /// operation and are not counted.
    pub const CX1: TearMix = TearMix {
        erase_zeroing: 17,
        erase_all_zero: 63,
        erase_erasing: 69,
        erase_reads_ff_weak: 4,
        erase_reads_ff: 264,
        cut_before_erase: 1,
        program_command_boundary: 58,
        program_mid_command: 19,
        program_mid_word: 2,
    };

    /// Only `shape` for every torn erase, the program weights unchanged: a
    /// sweep that wants every erase cut to meet one state. No erase cut
    /// lands before its erase.
    pub fn erase_only(self, shape: EraseShape) -> TearMix {
        let one = |s: EraseShape| u32::from(s == shape);
        TearMix {
            erase_zeroing: one(EraseShape::Zeroing),
            erase_all_zero: one(EraseShape::AllZero),
            erase_erasing: one(EraseShape::Erasing),
            erase_reads_ff_weak: one(EraseShape::ReadsFfWeak),
            erase_reads_ff: one(EraseShape::ReadsFf),
            cut_before_erase: 0,
            ..self
        }
    }

    /// Every erase cut lands before its erase starts, the program weights
    /// unchanged.
    pub fn before_erase_only(self) -> TearMix {
        TearMix {
            cut_before_erase: 1,
            ..self.without_erase_shapes()
        }
    }

    /// Only `shape` for every torn program, the erase weights unchanged.
    pub fn program_only(self, shape: ProgramShape) -> TearMix {
        let one = |s: ProgramShape| u32::from(s == shape);
        TearMix {
            program_command_boundary: one(ProgramShape::CommandBoundary),
            program_mid_command: one(ProgramShape::MidCommand),
            program_mid_word: one(ProgramShape::MidWord),
            ..self
        }
    }

    fn without_erase_shapes(self) -> TearMix {
        TearMix {
            erase_zeroing: 0,
            erase_all_zero: 0,
            erase_erasing: 0,
            erase_reads_ff_weak: 0,
            erase_reads_ff: 0,
            ..self
        }
    }

    /// Draw what a cut at an erase did: `None` when it landed before the
    /// erase started (the sector untouched), else the torn erase's shape.
    pub fn draw_erase_cut(&self, rng: &mut SimRng) -> Option<EraseShape> {
        let w = [
            (Some(EraseShape::Zeroing), self.erase_zeroing),
            (Some(EraseShape::AllZero), self.erase_all_zero),
            (Some(EraseShape::Erasing), self.erase_erasing),
            (Some(EraseShape::ReadsFfWeak), self.erase_reads_ff_weak),
            (Some(EraseShape::ReadsFf), self.erase_reads_ff),
            (None, self.cut_before_erase),
        ];
        pick(rng, &w).unwrap_or(Some(EraseShape::ReadsFf))
    }

    /// Draw a torn program's shape.
    pub fn draw_program(&self, rng: &mut SimRng) -> ProgramShape {
        let w = [
            (ProgramShape::CommandBoundary, self.program_command_boundary),
            (ProgramShape::MidCommand, self.program_mid_command),
            (ProgramShape::MidWord, self.program_mid_word),
        ];
        pick(rng, &w).unwrap_or(ProgramShape::CommandBoundary)
    }
}

impl Default for TearMix {
    fn default() -> Self {
        Self::CX1
    }
}

/// The 69 *erasing* cuts on CX1, as (stable zero bits left, weak bits),
/// sorted. A 4 KiB sector has 32,768 bits. The model draws a
/// point of this empirical distribution, linearly interpolated between
/// neighbours, so every draw lies inside what was observed.
pub const CX1_ERASING: [(u32, u32); 69] = [
    (1, 0),
    (1, 0),
    (1, 0),
    (1, 1),
    (1, 1),
    (1, 2),
    (1, 3),
    (1, 5),
    (1, 6),
    (1, 6),
    (2, 2),
    (3, 0),
    (3, 3),
    (4, 2),
    (4, 4),
    (6, 8),
    (6, 13),
    (10, 15),
    (17, 34),
    (18, 14),
    (22, 20),
    (28, 27),
    (32, 48),
    (34, 39),
    (40, 32),
    (42, 49),
    (43, 43),
    (54, 59),
    (58, 57),
    (76, 65),
    (76, 71),
    (82, 60),
    (86, 65),
    (97, 76),
    (114, 83),
    (121, 73),
    (126, 99),
    (132, 83),
    (148, 123),
    (182, 140),
    (213, 164),
    (215, 140),
    (221, 183),
    (226, 158),
    (310, 197),
    (318, 223),
    (324, 261),
    (336, 201),
    (488, 284),
    (498, 257),
    (529, 301),
    (557, 289),
    (576, 360),
    (670, 364),
    (703, 342),
    (725, 440),
    (918, 545),
    (1171, 555),
    (1341, 603),
    (1706, 761),
    (3214, 1117),
    (4278, 1423),
    (4951, 1350),
    (11556, 2343),
    (13273, 2166),
    (17358, 2249),
    (25545, 1486),
    (28664, 1030),
    (32197, 181),
];

/// Weak bits in the sectors that read all `0xFF` and still had some.
pub const CX1_READS_FF_WEAK: [u32; 4] = [1, 2, 3, 8];

/// The torn programs that stopped inside a word, as (clears of that word
/// landed, clears it asked for, weak bits), sorted by the landed share. A
/// draw interpolates the landed share and the weak count between
/// neighbours, as [`CX1_ERASING`] does, and applies them to the drawn word
/// (a word asking two or more clears; at least one lands, at least one does
/// not). In both cuts the weak bit sat on a clear that had not landed — in
/// the second, on the only one: that word reads exactly as asked on the
/// reads where its weak bit reads 0.
pub const CX1_MID_WORD: [(u32, u32, u32); 2] = [(3, 17, 1), (14, 15, 1)];

/// Tear an erase of `cells` (whose old content is still in it) into `shape`.
/// `weak` is the sector's weak mask, all zero on entry.
pub fn tear_erase(shape: EraseShape, rng: &mut SimRng, cells: &mut [u8], weak: &mut [u8]) {
    let bits = (cells.len() * 8) as u64;
    match shape {
        EraseShape::Zeroing => {
            // A word boundary strictly inside the sector.
            let len = cells.len();
            let words = (len / TEAR_WORD) as u64;
            let end = (1 + rng.below(words.saturating_sub(1).max(1))) as usize * TEAR_WORD;
            cells[..end.min(len)].fill(0x00);
        }
        EraseShape::AllZero => cells.fill(0x00),
        EraseShape::Erasing => {
            let (zeros, weaks) = interpolate(rng, &CX1_ERASING);
            // Exactly `zeros` stable zeros and `weaks` weak bits, at distinct
            // uniform positions (a partial Fisher-Yates over the bits); every
            // other bit lifted to 1. Exact counts, not per-bit chances: a
            // residue of one bit stays one bit, as it was on the part.
            cells.fill(0xFF);
            let n = bits as usize;
            let pick = (zeros + weaks).min(bits as u32) as usize;
            let mut order: Vec<u32> = (0..bits as u32).collect();
            for i in 0..pick {
                let j = i + rng.below((n - i) as u64) as usize;
                order.swap(i, j);
                let b = order[i] as usize;
                if i < zeros as usize {
                    cells[b / 8] &= !(1 << (b % 8));
                } else {
                    weak[b / 8] |= 1 << (b % 8);
                }
            }
        }
        EraseShape::ReadsFfWeak => {
            cells.fill(0xFF);
            let n = CX1_READS_FF_WEAK[rng.below(CX1_READS_FF_WEAK.len() as u64) as usize];
            for _ in 0..n {
                let b = rng.below(bits) as usize;
                weak[b / 8] |= 1 << (b % 8);
            }
        }
        EraseShape::ReadsFf => cells.fill(0xFF),
    }
}

/// Tear a program of `data` at `cells[off..]`: a prefix of whole 4-byte
/// words lands, ending on a 32-byte command boundary or inside a command —
/// or, [`ProgramShape::MidWord`], the next word lands only part of its
/// clears and takes weak bits (in `weak`, the sector's weak mask).
pub fn tear_program(
    mix: &TearMix,
    rng: &mut SimRng,
    cells: &mut [u8],
    weak: &mut [u8],
    off: usize,
    data: &[u8],
) {
    let ends = data.len().div_ceil(TEAR_WORD) as u64; // word ends in 0..len
    let commands = data.len().div_ceil(TEAR_COMMAND) as u64; // of them, on a command
    let mid = ends - commands;
    // A word caught part-way lands some of its clears and not all, so it
    // must ask for two or more: both silicon words did (17 and 15), and
    // both landed some (3 and 14). A word left with weak bits and no
    // landed clear would read erased on some reads; no cut showed one.
    let clears = |w: usize| -> Vec<usize> {
        let (lo, hi) = (w * TEAR_WORD, ((w + 1) * TEAR_WORD).min(data.len()));
        (lo * 8..hi * 8)
            .filter(|&b| (cells[off + b / 8] & !data[b / 8]) >> (b % 8) & 1 == 1)
            .collect()
    };
    let mut shape = mix.draw_program(rng);
    if shape == ProgramShape::MidCommand && mid == 0 {
        shape = ProgramShape::CommandBoundary;
    }
    let candidates: Vec<usize> = match shape {
        ProgramShape::MidWord => (0..ends as usize)
            .filter(|&w| clears(w).len() >= 2)
            .collect(),
        _ => Vec::new(),
    };
    if shape == ProgramShape::MidWord && candidates.is_empty() {
        shape = ProgramShape::CommandBoundary;
    }
    let end = match shape {
        ProgramShape::CommandBoundary => rng.below(commands) as usize * TEAR_COMMAND,
        ProgramShape::MidCommand => {
            // The k-th word end that is not a command end.
            let k = rng.below(mid) as usize;
            let per = TEAR_COMMAND / TEAR_WORD - 1;
            (k / per) * TEAR_COMMAND + (k % per + 1) * TEAR_WORD
        }
        ProgramShape::MidWord => {
            let w = candidates[rng.below(candidates.len() as u64) as usize];
            let mut bits = clears(w);
            let k = bits.len() as u32;
            // The landed share in 1/1024, interpolated with the weak count.
            let rows: Vec<(u32, u32)> = CX1_MID_WORD
                .iter()
                .map(|&(l, i, wk)| (l * 1024 / i.max(1), wk))
                .collect();
            let (share, weaks) = interpolate(rng, &rows);
            let landed = ((share * k + 512) / 1024).clamp(1, k - 1);
            let weaks = weaks.min(k - landed);
            for i in 0..(landed + weaks) as usize {
                let j = i + rng.below((bits.len() - i) as u64) as usize;
                bits.swap(i, j);
                let b = bits[i];
                if i < landed as usize {
                    cells[off + b / 8] &= !(1 << (b % 8));
                } else {
                    weak[off + b / 8] |= 1 << (b % 8);
                }
            }
            w * TEAR_WORD
        }
    };
    for (i, &d) in data.iter().enumerate().take(end) {
        cells[off + i] &= d;
    }
}

fn pick<T: Copy>(rng: &mut SimRng, weights: &[(T, u32)]) -> Option<T> {
    let total: u64 = weights.iter().map(|&(_, w)| w as u64).sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.below(total);
    for &(t, w) in weights {
        if r < w as u64 {
            return Some(t);
        }
        r -= w as u64;
    }
    None
}

/// A point of the empirical distribution `rows` (sorted by `.0`): a uniform
/// position along its `n - 1` segments, both coordinates interpolated.
fn interpolate(rng: &mut SimRng, rows: &[(u32, u32)]) -> (u32, u32) {
    if rows.len() < 2 {
        return rows.first().copied().unwrap_or((0, 0));
    }
    const STEPS: u64 = 1024;
    let u = rng.below((rows.len() as u64 - 1) * STEPS);
    let (i, f) = ((u / STEPS) as usize, (u % STEPS) as i64);
    let lerp = |a: u32, b: u32| (a as i64 + (b as i64 - a as i64) * f / STEPS as i64) as u32;
    (
        lerp(rows[i].0, rows[i + 1].0),
        lerp(rows[i].1, rows[i + 1].1),
    )
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    const SS: usize = 4096;

    fn old() -> Vec<u8> {
        let mut r = SimRng::new(99);
        (0..SS).map(|_| r.next_u8()).collect()
    }

    fn zero_bits(b: &[u8]) -> u32 {
        b.iter().map(|x| x.count_zeros()).sum()
    }

    /// `TearMix::CX1` with no sub-word tears: what a whole-word prefix
    /// test draws from.
    const WHOLE_WORDS: TearMix = TearMix {
        program_mid_word: 0,
        ..TearMix::CX1
    };

    #[test]
    fn every_erase_shape_is_what_it_says() {
        let old = old();
        for shape in [
            EraseShape::Zeroing,
            EraseShape::AllZero,
            EraseShape::Erasing,
            EraseShape::ReadsFfWeak,
            EraseShape::ReadsFf,
        ] {
            let mix = TearMix::CX1.erase_only(shape);
            for seed in 0..40 {
                let mut rng = SimRng::new(seed);
                assert_eq!(mix.draw_erase_cut(&mut rng), Some(shape));
                let mut cells = old.clone();
                let mut weak = vec![0u8; SS];
                tear_erase(shape, &mut rng, &mut cells, &mut weak);
                let weak_bits: u32 = weak.iter().map(|w| w.count_ones()).sum();
                match shape {
                    EraseShape::Zeroing => {
                        let end = cells.iter().position(|&b| b != 0).unwrap_or(SS);
                        // The run may run on into old zero bytes; it starts
                        // on or before the first word boundary past `end`.
                        assert!(end > 0 && end < SS, "seed {seed}: end {end}");
                        let cut = cells
                            .chunks(TEAR_WORD)
                            .position(|w| w != [0; TEAR_WORD])
                            .unwrap()
                            * TEAR_WORD;
                        assert_eq!(&cells[cut..], &old[cut..], "seed {seed}: old after the run");
                        assert_eq!(weak_bits, 0);
                    }
                    EraseShape::AllZero => {
                        assert!(cells.iter().all(|&b| b == 0));
                        assert_eq!(weak_bits, 0);
                    }
                    EraseShape::Erasing => {
                        let z = zero_bits(&cells);
                        assert!(z <= 32_197, "seed {seed}: {z}");
                        // Weak bits never sit on a stable zero.
                        assert!(cells.iter().zip(&weak).all(|(c, w)| c & w == *w));
                    }
                    EraseShape::ReadsFfWeak => {
                        assert!(cells.iter().all(|&b| b == 0xFF));
                        assert!((1..=8).contains(&weak_bits));
                    }
                    EraseShape::ReadsFf => {
                        assert!(cells.iter().all(|&b| b == 0xFF));
                        assert_eq!(weak_bits, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn erasing_spreads_its_zeros_over_old_and_new_positions_alike() {
        let old = old();
        let (mut on_old_zero, mut on_old_one) = (0u64, 0u64);
        for seed in 0..200 {
            let mut rng = SimRng::new(seed);
            let mut cells = old.clone();
            let mut weak = vec![0u8; SS];
            tear_erase(EraseShape::Erasing, &mut rng, &mut cells, &mut weak);
            for (c, o) in cells.iter().zip(&old) {
                on_old_zero += (!c & !o).count_ones() as u64;
                on_old_one += (!c & o).count_ones() as u64;
            }
        }
        let old_zero = zero_bits(&old) as f64;
        let old_one = (SS * 8) as f64 - old_zero;
        let (a, b) = (on_old_zero as f64 / old_zero, on_old_one as f64 / old_one);
        assert!((a / b - 1.0).abs() < 0.05, "shares {a} / {b}");
    }

    #[test]
    fn a_torn_program_is_a_prefix_of_whole_words() {
        let data = page_data();
        let mut seen_command = 0;
        let mut seen_mid = 0;
        for seed in 0..400 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; SS];
            let mut weak = vec![0u8; SS];
            tear_program(&WHOLE_WORDS, &mut rng, &mut cells, &mut weak, 512, &data);
            let page = &cells[512..768];
            let end = (0..=256)
                .find(|&n| page[..n] == data[..n] && page[n..].iter().all(|&b| b == 0xFF))
                .expect("a prefix, then nothing");
            assert!(end < 256);
            assert_eq!(end % TEAR_WORD, 0, "seed {seed}: end {end}");
            if end % TEAR_COMMAND == 0 {
                seen_command += 1;
            } else {
                seen_mid += 1;
            }
            assert!(cells[..512].iter().chain(&cells[768..]).all(|&b| b == 0xFF));
            assert!(
                weak.iter().all(|&w| w == 0),
                "seed {seed}: a whole-word tear is never weak"
            );
        }
        // 58 : 19 of 400 is 301 : 99; a binomial sd is 8.6.
        assert!(
            (270..=332).contains(&seen_command),
            "{seen_command} / {seen_mid}"
        );
    }

    #[test]
    fn a_mid_word_tear_lands_whole_words_then_part_of_one_word_then_nothing() {
        let data = page_data();
        let mix = TearMix::CX1.program_only(ProgramShape::MidWord);
        let mut landed_shares = Vec::new();
        for seed in 0..400 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; SS];
            let mut weak = vec![0u8; SS];
            tear_program(&mix, &mut rng, &mut cells, &mut weak, 512, &data);
            let page = &cells[512..768];
            let pw = &weak[512..768];
            let w = (0..256 / TEAR_WORD)
                .find(|&w| page[w * TEAR_WORD..][..TEAR_WORD] != data[w * TEAR_WORD..][..TEAR_WORD])
                .expect("one word does not land whole");
            let (lo, hi) = (w * TEAR_WORD, (w + 1) * TEAR_WORD);
            assert_eq!(page[..lo], data[..lo], "seed {seed}: whole words before it");
            assert!(
                page[hi..].iter().all(|&b| b == 0xFF),
                "seed {seed}: nothing after it"
            );
            assert!(
                pw[..lo].iter().chain(&pw[hi..]).all(|&x| x == 0),
                "seed {seed}: weak only in it"
            );
            // In the word: only clears it asked for, some of them; the weak
            // bits sit on clears that did not land.
            let asked: u32 = data[lo..hi].iter().map(|d| d.count_zeros()).sum();
            let landed: u32 = page[lo..hi].iter().map(|c| c.count_zeros()).sum();
            let weak_bits: u32 = pw[lo..hi].iter().map(|x| x.count_ones()).sum();
            assert!(
                page[lo..hi]
                    .iter()
                    .zip(&data[lo..hi])
                    .all(|(c, d)| c & d == *d),
                "no strays"
            );
            assert!(landed < asked, "seed {seed}: {landed} of {asked}");
            assert!(
                page[lo..hi]
                    .iter()
                    .zip(&pw[lo..hi])
                    .all(|(c, x)| c & x == *x)
            );
            assert!(
                pw[lo..hi]
                    .iter()
                    .zip(&data[lo..hi])
                    .all(|(x, d)| x & d == 0)
            );
            assert!(
                weak_bits >= 1 || landed + 1 >= asked,
                "seed {seed}: CX1's sub-word tears had a weak bit"
            );
            landed_shares.push(landed as f64 / asked as f64);
        }
        // Between the two observed shares, 3 / 17 and 14 / 15 (a word's few
        // clears round them).
        let mean = landed_shares.iter().sum::<f64>() / landed_shares.len() as f64;
        assert!((0.4..=0.7).contains(&mean), "mean landed share {mean}");
    }

    #[test]
    fn the_calibrated_mix_tears_a_program_inside_a_word_about_one_time_in_forty() {
        let data = page_data();
        let mut inside = 0;
        for seed in 0..4000 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; 256];
            let mut weak = vec![0u8; 256];
            tear_program(&TearMix::CX1, &mut rng, &mut cells, &mut weak, 0, &data);
            let whole =
                (0..=256).any(|n| cells[..n] == data[..n] && cells[n..].iter().all(|&b| b == 0xFF));
            inside += u32::from(!whole);
        }
        // 2 of 79: 101 of 4000, binomial sd 10.
        assert!((70..=135).contains(&inside), "{inside} of 4000");
    }

    #[test]
    fn a_short_program_still_tears_on_words() {
        for seed in 0..50 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; 64];
            let mut weak = vec![0u8; 64];
            tear_program(&WHOLE_WORDS, &mut rng, &mut cells, &mut weak, 0, &[0u8; 16]);
            let end = cells.iter().position(|&b| b != 0).unwrap();
            assert!(end < 16 && end % TEAR_WORD == 0, "seed {seed}: {end}");
        }
        let mut rng = SimRng::new(1);
        let mut cells = [0xFFu8; 4];
        let mut weak = [0u8; 4];
        tear_program(&WHOLE_WORDS, &mut rng, &mut cells, &mut weak, 0, &[0u8; 3]);
        assert_eq!(
            cells, [0xFF; 4],
            "a program shorter than a word lands nothing"
        );
        // A word that asks for no clear cannot be caught part-way: a sub-word
        // tear of a program that clears nothing falls back to a prefix.
        let mix = TearMix::CX1.program_only(ProgramShape::MidWord);
        let mut cells = [0x00u8; 16];
        let mut weak = [0u8; 16];
        tear_program(&mix, &mut rng, &mut cells, &mut weak, 0, &[0u8; 16]);
        assert_eq!((cells, weak), ([0u8; 16], [0u8; 16]));
    }

    #[test]
    fn the_shapes_come_in_the_observed_proportions() {
        let mut rng = SimRng::new(7);
        let mut n = [0u32; 6];
        for _ in 0..418_000 {
            let i = match TearMix::CX1.draw_erase_cut(&mut rng) {
                Some(shape) => shape as usize,
                None => 5,
            };
            n[i] += 1;
        }
        for (got, want) in n.iter().zip([17u32, 63, 69, 4, 264, 1]) {
            let want = want * 1000;
            assert!(got.abs_diff(want) <= want / 20 + 200, "{n:?}");
        }
    }

    #[test]
    fn the_flash_tears_through_the_calibrated_model_when_the_plan_names_it() {
        use crate::{FaultPlan, NorError, NorFlashSim, NorGeometry, TearModel};
        let mut zeroed = 0;
        for seed in 0..60 {
            let mut f = NorFlashSim::new(NorGeometry::new(2, SS as u32, 256));
            f.program(0, &old()).unwrap();
            f.set_plan(FaultPlan::cut(0, TearModel::Calibrated, seed));
            assert_eq!(f.erase_sector(0), Err(NorError::PowerLost));
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0u8; SS];
            f.peek(0, &mut b);
            if f.stats().erase_cuts_before_start == 1 {
                assert_eq!(b, old(), "seed {seed}: a cut before the erase leaves it");
                continue;
            }
            assert_ne!(b, old(), "seed {seed}: a started erase changes the sector");
            if b[..4] == [0; 4] {
                zeroed += 1;
            }
        }
        // 80 of 418 erase cuts read `0x00` at the front (zeroing or all zero).
        assert!((5..=22).contains(&zeroed), "{zeroed} of 60");

        let mut f = NorFlashSim::new(NorGeometry::new(2, SS as u32, 256));
        f.set_tear_mix(TearMix::CX1.erase_only(EraseShape::AllZero));
        f.set_plan(FaultPlan::cut(0, TearModel::Calibrated, 1));
        let _ = f.erase_sector(1);
        f.power_cycle(FaultPlan::none());
        let mut b = vec![0xAAu8; SS];
        f.read(SS as u32, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 0), "an all-zero tear reads 0x00");
        assert_eq!(f.sectors_in_use(), 1);
    }

    #[test]
    fn a_cut_before_the_erase_leaves_the_sector_as_it_was_and_untainted() {
        use crate::{FaultPlan, NorError, NorFlashSim, NorGeometry, TearModel};
        let tear = TearModel::from_name("calibrated_before_erase").unwrap();
        for seed in 0..20 {
            let mut f = NorFlashSim::new(NorGeometry::new(2, SS as u32, 256));
            f.program(0, &old()).unwrap();
            f.set_plan(FaultPlan::cut(0, tear, seed));
            assert_eq!(f.erase_sector(0), Err(NorError::PowerLost));
            assert!(!f.is_powered());
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0u8; SS];
            f.read(0, &mut b).unwrap();
            assert_eq!(b, old(), "seed {seed}");
            assert!(f.sector_damage(0).is_pristine(), "nothing touched it");
            let s = f.stats();
            assert_eq!(
                (s.torn_ops, s.torn_erases, s.erase_cuts_before_start),
                (1, 0, 1)
            );
            // A cut program still tears the calibrated way.
            f.set_plan(FaultPlan::cut(0, tear, seed));
            assert_eq!(f.program(SS as u32, &page_data()), Err(NorError::PowerLost));
        }
        // And CX1's mix lands one cut in 418 at an erase there.
        let mut before = 0;
        for seed in 0..8000 {
            let mut f = NorFlashSim::new(NorGeometry::new(1, SS as u32, 256));
            f.set_plan(FaultPlan::cut(0, TearModel::Calibrated, seed));
            let _ = f.erase_sector(0);
            before += f.stats().erase_cuts_before_start;
        }
        assert!((8..=36).contains(&before), "{before} of 8000");
    }

    #[test]
    fn every_named_model_round_trips_and_a_forced_shape_is_forced() {
        use crate::{FaultPlan, NorFlashSim, NorGeometry, TearModel};
        for t in TearModel::NAMED {
            assert_eq!(TearModel::from_name(t.name()), Some(t), "{}", t.name());
            assert_eq!(
                t.is_calibrated(),
                t.name().starts_with("calibrated"),
                "{}",
                t.name()
            );
        }
        assert!(TearModel::ALL.iter().all(|t| TearModel::NAMED.contains(t)));
        for seed in 0..20 {
            let mut f = NorFlashSim::new(NorGeometry::new(1, SS as u32, 256));
            f.program(0, &old()).unwrap();
            let tear = TearModel::from_name("calibrated_all_zero").unwrap();
            f.set_plan(FaultPlan::cut(0, tear, seed));
            let _ = f.erase_sector(0);
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0xAAu8; SS];
            f.read(0, &mut b).unwrap();
            assert!(b.iter().all(|&x| x == 0), "seed {seed}");
        }
        // calibrated_mid_word: every torn program is caught inside a word,
        // and its weak bits reach the flash's weak map.
        let tear = TearModel::from_name("calibrated_mid_word").unwrap();
        let mut weak_seen = 0;
        for seed in 0..20 {
            let mut f = NorFlashSim::new(NorGeometry::new(1, SS as u32, 256));
            f.set_plan(FaultPlan::cut(0, tear, seed));
            let _ = f.program(0, &page_data());
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0u8; 256];
            f.peek(0, &mut b);
            let data = page_data();
            assert!(
                !(0..=256).any(|n| b[..n] == data[..n] && b[n..].iter().all(|&x| x == 0xFF)),
                "seed {seed}: not a whole-word prefix"
            );
            weak_seen += f.sector_damage(0).weak_bits();
        }
        assert!(weak_seen > 0);
    }

    #[test]
    fn interpolation_stays_inside_the_observed_range() {
        let mut rng = SimRng::new(3);
        for _ in 0..10_000 {
            let (z, w) = interpolate(&mut rng, &CX1_ERASING);
            assert!((1..=32_197).contains(&z));
            assert!(w <= 2_343);
        }
    }

    fn page_data() -> Vec<u8> {
        (0..256u32).map(|i| (i * 37 + 1) as u8 & 0x7F).collect()
    }
}
