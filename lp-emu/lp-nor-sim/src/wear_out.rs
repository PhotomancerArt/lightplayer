//! Wear-out: a sector that stops taking erases or programs after it has been
//! erased a given number of times. An *injected* failure (nothing wears out
//! unless a test asks for it), additive to the tear model: with no
//! [`WearOut`] installed, every operation behaves exactly as before.
//!
//! Real NOR reports nothing when a cell wears out; the operation "succeeds"
//! and the cells are wrong. So does this model: a worn erase leaves a seeded
//! sprinkling of bits stuck at 0, a worn program leaves a seeded subset of
//! its intended 1→0 clears undone. Only a store that reads back what it wrote
//! can tell. A worn sector is marked *tainted* (`NorSectorState`), so a later
//! 0→1 program into its stuck bits is counted, never panicked on.

use alloc::vec::Vec;

use crate::SimRng;

/// What stops working when a sector wears out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WearMode {
    /// Every erase after the limit leaves some bits stuck at 0.
    EraseFails,
    /// Every program after the limit leaves some intended clears undone.
    ProgramFails,
}

/// One sector's wear-out plan: once it has been erased `after_erases` times
/// (counted from when the plan was installed), every later erase (or
/// program, per `mode`) goes wrong. `seed` picks the bad bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WearOut {
    pub sector: u32,
    pub after_erases: u64,
    pub mode: WearMode,
    pub seed: u64,
}

/// An installed plan and how far its sector has got.
#[derive(Clone, Debug)]
pub(crate) struct WearState {
    pub plan: WearOut,
    pub erases_seen: u64,
}

impl WearState {
    fn worn(&self) -> bool {
        self.erases_seen >= self.plan.after_erases
    }
}

/// A completed erase of `sector` (cells already all `0xFF`): count it, and
/// when the sector is worn, stick some bits at 0. Returns whether it did.
pub(crate) fn on_erase(wear: &mut [WearState], sector: u32, cells: &mut [u8]) -> bool {
    let mut damaged = false;
    for w in wear.iter_mut().filter(|w| w.plan.sector == sector) {
        if w.worn() && w.plan.mode == WearMode::EraseFails {
            let mut rng = SimRng::new(w.plan.seed ^ w.erases_seen);
            for c in cells.iter_mut() {
                if rng.chance(1, 64) {
                    *c &= !(1 << rng.below(8));
                    damaged = true;
                }
            }
        }
        w.erases_seen += 1;
    }
    damaged
}

/// A program of `data` at `sector`/`off`: when the sector is worn for
/// programs, the data with some of its 0 bits turned back to 1 (those clears
/// do not happen). `None` = program as asked.
pub(crate) fn on_program(
    wear: &[WearState],
    sector: u32,
    off: usize,
    data: &[u8],
) -> Option<Vec<u8>> {
    let w = wear
        .iter()
        .find(|w| w.plan.sector == sector && w.worn() && w.plan.mode == WearMode::ProgramFails)?;
    let mut rng = SimRng::new(w.plan.seed ^ (off as u64) << 20 ^ w.erases_seen);
    let mut out = data.to_vec();
    let mut changed = false;
    for b in out.iter_mut() {
        let clears = !*b;
        if clears != 0 && rng.chance(1, 4) {
            *b |= clears & rng.next_u8() | (1 << clears.trailing_zeros());
            changed = true;
        }
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use crate::{FaultPlan, NorFlashSim, NorGeometry, TearModel, WearMode, WearOut};
    use alloc::vec;

    fn plan(mode: WearMode) -> WearOut {
        WearOut {
            sector: 1,
            after_erases: 2,
            mode,
            seed: 7,
        }
    }

    #[test]
    fn erase_fails_after_the_limit_and_only_on_its_sector() {
        let mut f = NorFlashSim::new(NorGeometry::new(4, 4096, 256));
        f.add_wear_out(plan(WearMode::EraseFails));
        let mut b = vec![0u8; 4096];
        for _ in 0..2 {
            f.erase_sector(1).unwrap();
            f.read(4096, &mut b).unwrap();
            assert!(b.iter().all(|&x| x == 0xFF), "fine before the limit");
        }
        f.erase_sector(1).unwrap();
        f.read(4096, &mut b).unwrap();
        assert!(b.iter().any(|&x| x != 0xFF), "worn: some bits stuck at 0");
        f.erase_sector(2).unwrap();
        f.read(2 * 4096, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 0xFF), "other sectors untouched");
        // A program over stuck bits is counted, never panicked on.
        f.set_panic_on_violation(true);
        f.program(4096, &[0xFF; 256]).unwrap();
    }

    #[test]
    fn program_fails_after_the_limit() {
        let mut f = NorFlashSim::new(NorGeometry::new(4, 4096, 256));
        f.add_wear_out(plan(WearMode::ProgramFails));
        let data = [0u8; 64];
        let mut b = [0u8; 64];
        f.program(4096, &data).unwrap();
        f.read(4096, &mut b).unwrap();
        assert_eq!(b, data, "fine before any erase");
        f.erase_sector(1).unwrap();
        f.erase_sector(1).unwrap();
        f.program(4096, &data).unwrap();
        f.read(4096, &mut b).unwrap();
        assert_ne!(b, data, "worn: some clears did not happen");
    }

    #[test]
    fn no_wear_out_changes_nothing_about_cuts() {
        let run = |wear: bool| {
            let mut f = NorFlashSim::new(NorGeometry::new(4, 4096, 256));
            if wear {
                f.add_wear_out(WearOut {
                    sector: 3,
                    after_erases: 1000,
                    mode: WearMode::EraseFails,
                    seed: 1,
                });
            }
            f.set_plan(FaultPlan::cut(3, TearModel::RandomBits, 99));
            for i in 0..8u32 {
                if f.program(i * 300, &[0x11u8; 300]).is_err() {
                    break;
                }
            }
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0u8; 4096];
            f.read(0, &mut b).unwrap();
            b
        };
        assert_eq!(run(false), run(true));
    }
}
