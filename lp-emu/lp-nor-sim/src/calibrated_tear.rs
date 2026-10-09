//! The `calibrated` tear model: torn erases and programs shaped the way a
//! real part tore, in the proportions it tore them.
//!
//! **Source:** `docs/reports/2026-10-08-c6-nor-tear-calibration.md` — 200
//! power cuts on CX1 (`c6-expendable`, MAC `14:C1:9F:E6:54:90`, a generic
//! ESP32-C6 dev board), flash part JEDEC id `0x464016` (4 MiB), VBUS cut at
//! the board's USB hub, `silicon:esp32c6`, the `flash-tears` payload. **It is
//! calibrated on 200 cuts and is re-checked at 500**: the weights and the two
//! tables below are counts out of that report's generated section, and
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
//!   the erase was never finished. No cut left the old data whole, so this
//!   model never does either.
//! - **A program stops on the mask ROM's 32-byte command, or inside one on a
//!   4-byte word** ([`ProgramShape`]): a prefix of exactly what was asked,
//!   then nothing — no partial byte, no scatter, no weak bit. The command and
//!   word are counted from the start of the in-flight page operation (every
//!   silicon program was page-aligned, so absolute and relative alignment
//!   were not told apart).
//!
//! Where a cut lands — erase or program — is the workload's business (the
//! op counter), not the model's: only the shape *within* a torn operation is
//! drawn here. The silicon cut was uniform in time and 83 % of cuts landed in
//! an erase; a sweep cuts at every op instead.

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
    pub program_command_boundary: u32,
    pub program_mid_command: u32,
}

impl TearMix {
    /// CX1's 200 cuts: 166 erase cuts (10 zeroing, 26 all zero, 28 erasing,
    /// 1 reading `0xFF` with weak bits, 101 reading `0xFF`) and 33 torn
    /// programs (26 on a command boundary, 7 inside a command). The one cut
    /// that landed after a program finished is not a torn operation.
    pub const CX1: TearMix = TearMix {
        erase_zeroing: 10,
        erase_all_zero: 26,
        erase_erasing: 28,
        erase_reads_ff_weak: 1,
        erase_reads_ff: 101,
        program_command_boundary: 26,
        program_mid_command: 7,
    };

    /// Only `shape` for every torn erase, the program weights unchanged: a
    /// sweep that wants every erase cut to meet one state.
    pub fn erase_only(self, shape: EraseShape) -> TearMix {
        let one = |s: EraseShape| u32::from(s == shape);
        TearMix {
            erase_zeroing: one(EraseShape::Zeroing),
            erase_all_zero: one(EraseShape::AllZero),
            erase_erasing: one(EraseShape::Erasing),
            erase_reads_ff_weak: one(EraseShape::ReadsFfWeak),
            erase_reads_ff: one(EraseShape::ReadsFf),
            ..self
        }
    }

    /// Draw a torn erase's shape.
    pub fn draw_erase(&self, rng: &mut SimRng) -> EraseShape {
        let w = [
            (EraseShape::Zeroing, self.erase_zeroing),
            (EraseShape::AllZero, self.erase_all_zero),
            (EraseShape::Erasing, self.erase_erasing),
            (EraseShape::ReadsFfWeak, self.erase_reads_ff_weak),
            (EraseShape::ReadsFf, self.erase_reads_ff),
        ];
        pick(rng, &w).unwrap_or(EraseShape::ReadsFf)
    }

    /// Draw a torn program's shape.
    pub fn draw_program(&self, rng: &mut SimRng) -> ProgramShape {
        let w = [
            (ProgramShape::CommandBoundary, self.program_command_boundary),
            (ProgramShape::MidCommand, self.program_mid_command),
        ];
        pick(rng, &w).unwrap_or(ProgramShape::CommandBoundary)
    }
}

impl Default for TearMix {
    fn default() -> Self {
        Self::CX1
    }
}

/// The 28 *erasing* cuts on CX1, as (stable zero bits left, weak bits),
/// sorted. A 4 KiB sector has 32,768 bits. The model draws a
/// point of this empirical distribution, linearly interpolated between
/// neighbours, so every draw lies inside what was observed.
pub const CX1_ERASING: [(u32, u32); 28] = [
    (1, 0),
    (1, 1),
    (1, 2),
    (1, 6),
    (3, 0),
    (3, 3),
    (4, 2),
    (6, 8),
    (18, 14),
    (32, 48),
    (40, 32),
    (58, 57),
    (114, 83),
    (121, 73),
    (126, 99),
    (132, 83),
    (215, 140),
    (226, 158),
    (498, 257),
    (557, 289),
    (703, 342),
    (1171, 555),
    (1341, 603),
    (3214, 1117),
    (4278, 1423),
    (4951, 1350),
    (13273, 2166),
    (25545, 1486),
];

/// Weak bits in the sectors that read all `0xFF` and still had some: one
/// sector so far.
pub const CX1_READS_FF_WEAK: [u32; 1] = [2];

/// Tear an erase of `cells` (whose old content is still in it). `weak` is the
/// sector's weak mask, all zero on entry.
pub fn tear_erase(mix: &TearMix, rng: &mut SimRng, cells: &mut [u8], weak: &mut [u8]) {
    let bits = (cells.len() * 8) as u64;
    match mix.draw_erase(rng) {
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
/// words lands, ending on a 32-byte command boundary or inside a command.
pub fn tear_program(mix: &TearMix, rng: &mut SimRng, cells: &mut [u8], off: usize, data: &[u8]) {
    let ends = data.len().div_ceil(TEAR_WORD) as u64; // word ends in 0..len
    let commands = data.len().div_ceil(TEAR_COMMAND) as u64; // of them, on a command
    let mid = ends - commands;
    let shape = if mid == 0 {
        ProgramShape::CommandBoundary
    } else {
        mix.draw_program(rng)
    };
    let end = match shape {
        ProgramShape::CommandBoundary => rng.below(commands) as usize * TEAR_COMMAND,
        ProgramShape::MidCommand => {
            // The k-th word end that is not a command end.
            let k = rng.below(mid) as usize;
            let per = TEAR_COMMAND / TEAR_WORD - 1;
            (k / per) * TEAR_COMMAND + (k % per + 1) * TEAR_WORD
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
                let mut cells = old.clone();
                let mut weak = vec![0u8; SS];
                tear_erase(&mix, &mut rng, &mut cells, &mut weak);
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
                        assert!(z <= 27_000, "seed {seed}: {z}");
                        // Weak bits never sit on a stable zero.
                        assert!(cells.iter().zip(&weak).all(|(c, w)| c & w == *w));
                    }
                    EraseShape::ReadsFfWeak => {
                        assert!(cells.iter().all(|&b| b == 0xFF));
                        assert!((1..=2).contains(&weak_bits));
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
        let mix = TearMix::CX1.erase_only(EraseShape::Erasing);
        let (mut on_old_zero, mut on_old_one) = (0u64, 0u64);
        for seed in 0..200 {
            let mut rng = SimRng::new(seed);
            let mut cells = old.clone();
            let mut weak = vec![0u8; SS];
            tear_erase(&mix, &mut rng, &mut cells, &mut weak);
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
        let data: Vec<u8> = (0..256u32).map(|i| (i * 37 + 1) as u8 & 0x7F).collect();
        let mut seen_command = 0;
        let mut seen_mid = 0;
        for seed in 0..400 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; SS];
            tear_program(&TearMix::CX1, &mut rng, &mut cells, 512, &data);
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
        }
        // 26 : 7 of 400.
        assert!(
            (280..=345).contains(&seen_command),
            "{seen_command} / {seen_mid}"
        );
    }

    #[test]
    fn a_short_program_still_tears_on_words() {
        for seed in 0..50 {
            let mut rng = SimRng::new(seed);
            let mut cells = vec![0xFFu8; 64];
            tear_program(&TearMix::CX1, &mut rng, &mut cells, 0, &[0u8; 16]);
            let end = cells.iter().position(|&b| b != 0).unwrap();
            assert!(end < 16 && end % TEAR_WORD == 0, "seed {seed}: {end}");
        }
        let mut rng = SimRng::new(1);
        let mut cells = [0xFFu8; 4];
        tear_program(&TearMix::CX1, &mut rng, &mut cells, 0, &[0u8; 3]);
        assert_eq!(
            cells, [0xFF; 4],
            "a program shorter than a word lands nothing"
        );
    }

    #[test]
    fn the_shapes_come_in_the_observed_proportions() {
        let mut rng = SimRng::new(7);
        let mut n = [0u32; 5];
        for _ in 0..166_000 {
            n[TearMix::CX1.draw_erase(&mut rng) as usize] += 1;
        }
        for (got, want) in n.iter().zip([10u32, 26, 28, 1, 101]) {
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
            assert_ne!(b, old(), "seed {seed}: a started erase changes the sector");
            if b[..4] == [0; 4] {
                zeroed += 1;
            }
        }
        // 36 of 166 erase cuts read `0x00` at the front (zeroing or all zero).
        assert!((5..=25).contains(&zeroed), "{zeroed} of 60");

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
    fn every_named_model_round_trips_and_a_forced_shape_is_forced() {
        use crate::{FaultPlan, NorFlashSim, NorGeometry, TearModel};
        for t in TearModel::NAMED {
            assert_eq!(TearModel::from_name(t.name()), Some(t), "{}", t.name());
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
    }

    #[test]
    fn interpolation_stays_inside_the_observed_range() {
        let mut rng = SimRng::new(3);
        for _ in 0..10_000 {
            let (z, w) = interpolate(&mut rng, &CX1_ERASING);
            assert!((1..=25_545).contains(&z));
            assert!(w <= 2_166);
        }
    }
}
