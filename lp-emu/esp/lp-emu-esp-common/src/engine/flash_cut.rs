//! Power cuts at flash operations: the supply goes in the middle of one
//! program or erase command, and the command tears the way a NOR part tears.
//!
//! A [`FlashCut`] is a plan: an address range, the 0-based index `at` of the
//! in-range command to cut, a [`TearModel`] and a seed. The chip
//! ([`FlashImage`](super::spi_flash::FlashImage)) holds it; the command
//! engine ([`FlashEngine`](super::spi_flash::FlashEngine)) asks it, once per
//! program or erase the part would execute, whether this one runs.
//!
//! # What is counted (plan Q7)
//!
//! One counted operation is **one SPI program or erase command the part
//! executes whose target touches the range**: a page program (at most the
//! controller's 64-byte buffer; the C6's mask ROM sends 32-byte commands),
//! a 4 KiB sector erase, a 64 KiB block erase. Reads, write-enables, status
//! polls and status writes are not counted, and neither is a command the
//! part ignores (write-enable latch clear, or off the end of the chip). A
//! chip erase is never counted and never cut. `at` is 0-based and counts
//! from the moment the plan is armed: `at = 0` cuts the first in-range
//! program or erase, and `at` commands complete before the cut.
//!
//! # How the command tears (`lp-nor-sim`'s models)
//!
//! - `clean`: the command does nothing at all.
//! - `byte_prefix`, `random_bits`: the guessed models, applied by
//!   `lp-nor-sim`'s own [`NorFlashSim`] on a scratch copy of the cells, so
//!   their shapes are that crate's and nothing here re-derives them.
//! - `calibrated` and the five forced `calibrated_*` models:
//!   [`calibrated_tear::tear_program`] / [`calibrated_tear::tear_erase`]
//!   called directly on the chip's bytes with `SimRng::new(seed)`, the CX1
//!   mix ([`TearMix::CX1`], or [`TearMix::erase_only`] for a forced shape).
//!
//! nor-sim's "in-flight page program" is **this emulator's in-flight SPI
//! program command**. The calibrated model stops a program on a 32-byte ROM
//! command (`CommandBoundary`) or on a 4-byte word inside one
//! (`MidCommand`). The C6's ROM sends one 32-byte command per SPI
//! transaction, so `CommandBoundary` means the cut fell between commands —
//! nothing of this one landed, and every earlier one fully did — and
//! `MidCommand` is a whole-word prefix of this one. A 64-byte command (the
//! controller's buffer, if a driver ever sends one) can also stop at its
//! 32-byte midpoint.
//!
//! A **block erase** tears as one erase unit over its 64 KiB: the
//! calibrated physics (pre-program to `0x00` front to back, then lift every
//! cell at once) is the same, but the CX1 counts were measured on 4 KiB
//! sector erases only, so a torn block's residue is the sector table's
//! drawn over sixteen times the cells. Unmeasured; say so before quoting it.
//!
//! # After the cut
//!
//! The chip latches **powered off**: every later program, erase and chip
//! erase is refused and changes nothing, until
//! [`FlashImage::restore_power`](super::spi_flash::FlashImage::restore_power)
//! (a machine's power cycle). The plan is spent: it cuts once. The engine
//! posts [`MachineRequest::PowerCut`](crate::MachineRequest::PowerCut) and
//! yields, so the guest runs no further instruction — no "saved" reply can
//! leave a board that lost power — and the machine decides whether the run
//! stops or the board is power-cycled. Weak bits a torn erase left
//! ([`flash_weak_bits`](super::flash_weak_bits)) survive the power cycle.
//!
//! Nothing here reads a clock or an OS random source: the seed is the
//! caller's, so a cut is a function of `(image, workload, at, model, seed)`.

use core::fmt;
use core::ops::Range;

use lp_nor_sim::calibrated_tear;
use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry, SimRng, TearMix};

/// The tear models and erase shapes, re-exported so a chip crate and its
/// hosts name them without a second dependency.
pub use lp_nor_sim::{EraseShape, TearModel};

use super::flash_op_census::overlaps;

/// A power-cut plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashCut {
    /// Flash byte addresses a command must touch to be counted (and cut).
    pub range: Range<u32>,
    /// The 0-based index of the in-range command to cut, counted from when
    /// the plan was armed.
    pub at: u64,
    pub tear: TearModel,
    /// Every random choice of the tear (and the weak reads after it) comes
    /// from here.
    pub seed: u64,
}

/// A program or erase command, as the census and the cut count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlashOpKind {
    Program,
    SectorErase,
    BlockErase,
}

impl FlashOpKind {
    pub const fn name(self) -> &'static str {
        match self {
            FlashOpKind::Program => "program",
            FlashOpKind::SectorErase => "sector-erase",
            FlashOpKind::BlockErase => "block-erase",
        }
    }

    pub const fn is_erase(self) -> bool {
        !matches!(self, FlashOpKind::Program)
    }
}

impl fmt::Display for FlashOpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a cut did: the command it tore, how, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashCutReport {
    /// The plan's `at`: the 0-based in-range index of the torn command.
    pub index: u64,
    pub kind: FlashOpKind,
    /// The command's address (an erase's granule base).
    pub addr: u32,
    /// Bytes the program carried, or the erase granule.
    pub len: u32,
    pub tear: TearModel,
    /// The shape a calibrated erase tore into; `None` for a program, a
    /// `clean` cut, and the guessed models (whose erase shapes `lp-nor-sim`
    /// draws inside itself).
    pub erase_shape: Option<EraseShape>,
    /// Guest cycle of the command.
    pub cycle: u64,
    pub range_start: u32,
    pub range_len: u32,
    pub seed: u64,
}

impl fmt::Display for FlashCutReport {
    /// One parseable line:
    /// `FLASH-CUT op=<index> kind=<program|sector-erase|block-erase>
    /// addr=0x… len=<n> model=<name> shape=<shape|-> cycle=<n>
    /// range=0x…+0x… seed=<n>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FLASH-CUT op={} kind={} addr={:#x} len={} model={} shape={} cycle={} \
             range={:#x}+{:#x} seed={}",
            self.index,
            self.kind,
            self.addr,
            self.len,
            self.tear.name(),
            self.erase_shape.map_or("-", erase_shape_name),
            self.cycle,
            self.range_start,
            self.range_len,
            self.seed
        )
    }
}

/// An erase shape's name in the cut line (the forced models' suffixes).
pub fn erase_shape_name(shape: EraseShape) -> &'static str {
    match shape {
        EraseShape::Zeroing => "zeroing",
        EraseShape::AllZero => "all_zero",
        EraseShape::Erasing => "erasing",
        EraseShape::ReadsFfWeak => "reads_ff_weak",
        EraseShape::ReadsFf => "reads_ff",
    }
}

/// The engine's answer for one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutGate {
    /// Run it, atomically, as always.
    Run,
    /// The chip has no power: the command does nothing.
    Refused,
    /// Tear it per `plan`; it is the plan's `at`.
    Cut(FlashCut),
}

/// The chip's cut state: the armed plan, the count toward it, and the latch.
/// Inert by default (no plan, powered).
#[derive(Clone, Debug, Default)]
pub struct FlashCutter {
    plan: Option<FlashCut>,
    /// In-range commands seen since the plan was armed.
    seen: u64,
    powered_off: bool,
    last: Option<FlashCutReport>,
    /// Commands refused while powered off.
    refused: u64,
}

impl FlashCutter {
    /// Arm `plan` (replacing any plan not yet fired); its `at` counts from
    /// now.
    pub fn arm(&mut self, plan: FlashCut) {
        self.plan = Some(plan);
        self.seen = 0;
    }

    /// Drop an armed plan that has not fired.
    pub fn disarm(&mut self) {
        self.plan = None;
    }

    /// The plan still waiting to fire.
    pub fn armed(&self) -> Option<&FlashCut> {
        self.plan.as_ref()
    }

    /// In-range commands counted toward the armed plan.
    pub fn seen(&self) -> u64 {
        self.seen
    }

    pub fn is_powered_off(&self) -> bool {
        self.powered_off
    }

    /// The last cut that fired.
    pub fn last_cut(&self) -> Option<&FlashCutReport> {
        self.last.as_ref()
    }

    /// Commands refused since the last cut.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// The supply is back: commands run again. The cells, weak bits
    /// included, are what the cut left.
    pub fn restore_power(&mut self) {
        self.powered_off = false;
        self.refused = 0;
    }

    /// Decide one command the part would execute.
    pub(crate) fn gate(&mut self, addr: u32, len: u32) -> CutGate {
        if self.powered_off {
            self.refused += 1;
            return CutGate::Refused;
        }
        let Some(plan) = self.plan.as_ref() else {
            return CutGate::Run;
        };
        if !overlaps(&plan.range, addr, len) {
            return CutGate::Run;
        }
        let index = self.seen;
        self.seen += 1;
        if index != plan.at {
            return CutGate::Run;
        }
        self.powered_off = true;
        CutGate::Cut(self.plan.take().expect("the plan is armed"))
    }

    /// Refuse a command the cut is not counting (a chip erase) while
    /// powered off. `true` when it was refused.
    pub(crate) fn refuse_when_off(&mut self) -> bool {
        if self.powered_off {
            self.refused += 1;
        }
        self.powered_off
    }

    pub(crate) fn record(&mut self, report: FlashCutReport) {
        self.last = Some(report);
    }
}

/// The mix a calibrated `tear` draws from; `None` for the guessed models.
fn calibrated_mix(tear: TearModel) -> Option<TearMix> {
    match tear.forced_erase_shape() {
        Some(shape) => Some(TearMix::CX1.erase_only(shape)),
        None if tear.is_calibrated() => Some(TearMix::CX1),
        None => None,
    }
}

/// Tear a program of `data` into `cells` (the cells it targets, same
/// length). The cells' current content is what the program lands on.
pub(crate) fn tear_program(tear: TearModel, seed: u64, cells: &mut [u8], data: &[u8]) {
    if tear == TearModel::Clean || data.is_empty() {
        return;
    }
    match calibrated_mix(tear) {
        Some(mix) => {
            calibrated_tear::tear_program(&mix, &mut SimRng::new(seed), cells, 0, data);
        }
        None => {
            // The guessed models, exactly as `NorFlashSim` tears a page: one
            // page the length of this command, holding these cells.
            let len = cells.len() as u32;
            let mut sim = scratch(cells, len);
            sim.set_plan(FaultPlan::cut(0, tear, seed));
            let _ = sim.program(0, data);
            sim.peek(0, cells);
        }
    }
}

/// Tear an erase of `cells` (the whole granule, old content in it). New weak
/// bits are ORed into `weak` (the granule's mask, same length). The
/// calibrated erase shape, when there is one.
pub(crate) fn tear_erase(
    tear: TearModel,
    seed: u64,
    cells: &mut [u8],
    weak: &mut [u8],
) -> Option<EraseShape> {
    if tear == TearModel::Clean {
        return None;
    }
    match calibrated_mix(tear) {
        Some(mix) => {
            let mut rng = SimRng::new(seed);
            // The shape `tear_erase` will draw: the same first draw, on a
            // copy of the stream.
            let shape = mix.draw_erase(&mut rng.clone());
            calibrated_tear::tear_erase(&mix, &mut rng, cells, weak);
            Some(shape)
        }
        None => {
            let len = cells.len() as u32;
            let mut sim = scratch(cells, 256.min(len));
            sim.set_plan(FaultPlan::cut(0, tear, seed));
            let _ = sim.erase_sector(0);
            sim.peek(0, cells);
            if let Some(mask) = &sim.sector_damage(0).weak {
                for (w, m) in weak.iter_mut().zip(mask) {
                    *w |= m;
                }
            }
            None
        }
    }
}

/// A one-sector `NorFlashSim` holding `cells`, with `page` as its program
/// page.
fn scratch(cells: &[u8], page: u32) -> NorFlashSim {
    let len = cells.len() as u32;
    let mut sim = NorFlashSim::new(NorGeometry::new(1, len, page.max(1)));
    // The cells are whatever the chip holds, torn or not: laying them down
    // is not a store writing them.
    sim.set_panic_on_violation(false);
    sim.program(0, cells)
        .expect("a scratch part the size of its cells takes them");
    sim
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

    fn plan(at: u64) -> FlashCut {
        FlashCut {
            range: 0x1000..0x3000,
            at,
            tear: TearModel::Clean,
            seed: 1,
        }
    }

    #[test]
    fn the_cut_counts_only_in_range_commands_and_fires_once() {
        let mut c = FlashCutter::default();
        c.arm(plan(2));
        assert_eq!(c.gate(0x0000, 32), CutGate::Run, "out of range");
        assert_eq!(c.gate(0x1000, 32), CutGate::Run); // index 0
        assert_eq!(c.gate(0x3000, 4096), CutGate::Run, "out of range");
        assert_eq!(c.gate(0x2000, 4096), CutGate::Run); // index 1
        assert_eq!(c.seen(), 2);
        assert!(matches!(c.gate(0x1020, 32), CutGate::Cut(p) if p.at == 2));
        assert!(c.is_powered_off());
        assert!(c.armed().is_none(), "the plan is spent");
        // The latch refuses everything, in range or not.
        assert_eq!(c.gate(0x0000, 32), CutGate::Refused);
        assert_eq!(c.gate(0x1000, 32), CutGate::Refused);
        assert!(c.refuse_when_off());
        assert_eq!(c.refused(), 3);
        c.restore_power();
        assert_eq!(c.gate(0x1000, 32), CutGate::Run, "power is back, no plan");
        assert!(!c.refuse_when_off());
    }

    #[test]
    fn an_inert_cutter_runs_everything() {
        let mut c = FlashCutter::default();
        for addr in [0, 0x1000, 0x35_0000] {
            assert_eq!(c.gate(addr, 4096), CutGate::Run);
        }
        assert_eq!(c.seen(), 0);
    }

    #[test]
    fn a_clean_cut_changes_nothing() {
        let mut cells = old();
        let mut weak = vec![0u8; SS];
        assert_eq!(tear_erase(TearModel::Clean, 3, &mut cells, &mut weak), None);
        assert_eq!(cells, old());
        tear_program(TearModel::Clean, 3, &mut cells[..32], &[0u8; 32]);
        assert_eq!(cells, old());
        assert!(weak.iter().all(|&w| w == 0));
    }

    #[test]
    fn a_calibrated_tear_is_the_nor_sim_call_with_the_same_seed() {
        for seed in 0..40 {
            let data: Vec<u8> = (0..32u8).map(|i| i.wrapping_mul(37)).collect();
            let mut ours = vec![0xFFu8; 32];
            tear_program(TearModel::Calibrated, seed, &mut ours, &data);
            let mut theirs = vec![0xFFu8; 32];
            calibrated_tear::tear_program(
                &TearMix::CX1,
                &mut SimRng::new(seed),
                &mut theirs,
                0,
                &data,
            );
            assert_eq!(ours, theirs, "seed {seed}: program");

            let (mut ours, mut our_weak) = (old(), vec![0u8; SS]);
            let shape = tear_erase(TearModel::Calibrated, seed, &mut ours, &mut our_weak);
            let (mut theirs, mut their_weak) = (old(), vec![0u8; SS]);
            calibrated_tear::tear_erase(
                &TearMix::CX1,
                &mut SimRng::new(seed),
                &mut theirs,
                &mut their_weak,
            );
            assert_eq!(ours, theirs, "seed {seed}: erase cells");
            assert_eq!(our_weak, their_weak, "seed {seed}: erase weak bits");
            assert_eq!(
                shape,
                Some(TearMix::CX1.draw_erase(&mut SimRng::new(seed))),
                "seed {seed}: the reported shape is the one drawn"
            );
        }
    }

    #[test]
    fn a_forced_model_forces_its_shape_on_every_erase() {
        for (tear, shape) in [
            (TearModel::CalibratedZeroing, EraseShape::Zeroing),
            (TearModel::CalibratedAllZero, EraseShape::AllZero),
            (TearModel::CalibratedErasing, EraseShape::Erasing),
            (TearModel::CalibratedReadsFfWeak, EraseShape::ReadsFfWeak),
            (TearModel::CalibratedReadsFf, EraseShape::ReadsFf),
        ] {
            for seed in 0..20 {
                let (mut cells, mut weak) = (old(), vec![0u8; SS]);
                assert_eq!(tear_erase(tear, seed, &mut cells, &mut weak), Some(shape));
                let weak_bits: u32 = weak.iter().map(|w| w.count_ones()).sum();
                match shape {
                    EraseShape::AllZero => assert!(cells.iter().all(|&b| b == 0)),
                    EraseShape::ReadsFf => {
                        assert!(cells.iter().all(|&b| b == 0xFF));
                        assert_eq!(weak_bits, 0);
                    }
                    EraseShape::ReadsFfWeak => {
                        assert!(cells.iter().all(|&b| b == 0xFF));
                        assert!(weak_bits > 0, "{}: reads FF with weak bits", tear.name());
                    }
                    EraseShape::Zeroing => {
                        assert_eq!(cells[0], 0, "a zero run from the front");
                        assert_eq!(weak_bits, 0);
                    }
                    EraseShape::Erasing => {
                        assert_ne!(cells, old());
                    }
                }
            }
        }
    }

    #[test]
    fn a_byte_prefix_program_lands_a_prefix_and_random_bits_only_clear() {
        let data: Vec<u8> = (0..32u8).map(|i| !(i | 0x01)).collect();
        for seed in 0..60 {
            let mut cells = vec![0xFFu8; 32];
            tear_program(TearModel::BytePrefix, seed, &mut cells, &data);
            // The first n bytes landed whole, byte n is between, the rest is
            // untouched.
            // (Byte n may land whole by chance, so n is where the first
            // difference is, not the model's own n.)
            let n = cells.iter().zip(&data).take_while(|(c, d)| c == d).count();
            if n < 32 {
                assert_eq!(cells[n] & data[n], data[n], "byte {n} only lost clears");
                assert!(
                    cells[n + 1..].iter().all(|&b| b == 0xFF),
                    "seed {seed}: nothing after byte {n}"
                );
            }

            let old = old();
            let mut cells = old[..32].to_vec();
            tear_program(TearModel::RandomBits, seed, &mut cells, &data);
            for ((c, o), d) in cells.iter().zip(&old[..32]).zip(&data) {
                assert_eq!(c & !o, 0, "seed {seed}: no bit went 0→1");
                assert_eq!(!c & (o & d), 0, "seed {seed}: only intended clears");
            }
        }
    }

    #[test]
    fn a_guessed_erase_tears_and_may_leave_weak_bits() {
        let mut saw_weak = false;
        for tear in [TearModel::BytePrefix, TearModel::RandomBits] {
            for seed in 0..30 {
                let (mut cells, mut weak) = (old(), vec![0u8; SS]);
                assert_eq!(tear_erase(tear, seed, &mut cells, &mut weak), None);
                assert_ne!(cells, old(), "{} seed {seed}: the erase began", tear.name());
                saw_weak |= weak.iter().any(|&w| w != 0);
            }
        }
        assert!(saw_weak, "some guessed tears leave weak bits");
    }

    #[test]
    fn the_cut_line_names_everything_a_replay_needs() {
        let report = FlashCutReport {
            index: 12,
            kind: FlashOpKind::SectorErase,
            addr: 0x35_3000,
            len: 4096,
            tear: TearModel::Calibrated,
            erase_shape: Some(EraseShape::AllZero),
            cycle: 1_234,
            range_start: 0x35_0000,
            range_len: 0xB_0000,
            seed: 7,
        };
        assert_eq!(
            report.to_string(),
            "FLASH-CUT op=12 kind=sector-erase addr=0x353000 len=4096 model=calibrated \
             shape=all_zero cycle=1234 range=0x350000+0xb0000 seed=7"
        );
    }
}
