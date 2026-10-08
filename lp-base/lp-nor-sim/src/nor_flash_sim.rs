//! The flash itself: cells, the operation counter, the cut, the tears.

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::{FaultPlan, NorError, NorGeometry, NorSectorState, NorStats, SimRng, TearModel};

/// A NOR part that can lose power after any operation.
///
/// Cells live in one reference-counted buffer per sector, so `clone` is cheap
/// (a sweep runs a workload's prefix once and forks it for every cut point)
/// and a write copies only the sector it touches.
#[derive(Clone, Debug)]
pub struct NorFlashSim {
    geom: NorGeometry,
    sectors: Vec<Arc<Vec<u8>>>,
    damage: Vec<NorSectorState>,
    plan: FaultPlan,
    ops_since_plan: u64,
    powered: bool,
    tear_rng: SimRng,
    read_rng: SimRng,
    stats: NorStats,
    read_budget: Option<u64>,
    reads_since_cycle: u64,
    panic_on_violation: bool,
}

/// What `begin_op` decided about the operation about to run.
enum OpFate {
    Run,
    Tear,
}

impl NorFlashSim {
    /// A part with every sector erased (all `0xFF`).
    pub fn new(geom: NorGeometry) -> Self {
        Self::filled(geom, 0xFF)
    }

    /// A part whose every byte holds `byte` (e.g. `0x00` for "never erased").
    pub fn filled(geom: NorGeometry, byte: u8) -> Self {
        let sector = Arc::new(vec![byte; geom.sector_size as usize]);
        Self {
            geom,
            sectors: vec![sector; geom.sector_count as usize],
            damage: vec![NorSectorState::default(); geom.sector_count as usize],
            plan: FaultPlan::none(),
            ops_since_plan: 0,
            powered: true,
            tear_rng: SimRng::new(0),
            read_rng: SimRng::new(0x5EED_0F_8EAD),
            stats: NorStats::new(geom.sector_count),
            read_budget: None,
            reads_since_cycle: 0,
            panic_on_violation: cfg!(debug_assertions),
        }
    }

    /// A part full of seeded random bytes: "garbage flash" for format tests.
    pub fn garbage(geom: NorGeometry, seed: u64) -> Self {
        let mut sim = Self::new(geom);
        let mut rng = SimRng::new(seed);
        for s in &mut sim.sectors {
            let cells = Arc::make_mut(s);
            for b in cells.iter_mut() {
                *b = rng.next_u8();
            }
        }
        sim
    }

    pub fn geometry(&self) -> NorGeometry {
        self.geom
    }

    pub fn stats(&self) -> &NorStats {
        &self.stats
    }

    /// Zero the counters (cells, damage and plan are untouched).
    pub fn reset_stats(&mut self) {
        self.stats = NorStats::new(self.geom.sector_count);
    }

    pub fn plan(&self) -> FaultPlan {
        self.plan
    }

    /// Install a plan; its `cut_after` counts operations from now.
    pub fn set_plan(&mut self, plan: FaultPlan) {
        self.plan = plan;
        self.ops_since_plan = 0;
        self.tear_rng = SimRng::new(plan.seed ^ 0x7EA2_7EA2_7EA2_7EA2);
        self.read_rng = SimRng::new(plan.seed ^ 0x4EAD_4EAD_4EAD_4EAD);
    }

    /// Operations completed (or torn) since the plan was installed.
    pub fn ops_since_plan(&self) -> u64 {
        self.ops_since_plan
    }

    pub fn is_powered(&self) -> bool {
        self.powered
    }

    /// Restore power with the cells as they are (weak sectors stay weak) and
    /// install `next` (use [`FaultPlan::none`] for a fault-free run, or a new
    /// cut for a double cut).
    pub fn power_cycle(&mut self, next: FaultPlan) {
        self.powered = true;
        self.reads_since_cycle = 0;
        self.set_plan(next);
    }

    /// Fail reads with [`NorError::Watchdog`] after `budget` read calls since
    /// the last power cycle (`None` = unlimited).
    pub fn set_read_budget(&mut self, budget: Option<u64>) {
        self.read_budget = budget;
    }

    /// Panic on a 0→1 program into a pristine sector (default: debug builds).
    pub fn set_panic_on_violation(&mut self, panic: bool) {
        self.panic_on_violation = panic;
    }

    pub fn sector_damage(&self, sector: u32) -> &NorSectorState {
        &self.damage[sector as usize]
    }

    /// Read `buf.len()` bytes at `addr`. Weak bits read as fresh random values.
    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), NorError> {
        if !self.powered {
            return Err(NorError::PowerLost);
        }
        self.check_range(addr, buf.len())?;
        self.reads_since_cycle += 1;
        if let Some(budget) = self.read_budget
            && self.reads_since_cycle > budget
        {
            return Err(NorError::Watchdog);
        }
        self.stats.read_calls += 1;
        self.stats.read_bytes += buf.len() as u64;
        let ss = self.geom.sector_size as usize;
        let mut done = 0;
        while done < buf.len() {
            let a = addr as usize + done;
            let (sector, off) = (a / ss, a % ss);
            let n = (ss - off).min(buf.len() - done);
            let out = &mut buf[done..done + n];
            out.copy_from_slice(&self.sectors[sector][off..off + n]);
            if let Some(weak) = &self.damage[sector].weak {
                for (i, b) in out.iter_mut().enumerate() {
                    let m = weak[off + i];
                    if m != 0 {
                        *b = (*b & !m) | (self.read_rng.next_u8() & m);
                    }
                }
            }
            done += n;
        }
        Ok(())
    }

    /// Read cells without side effects (no stats, no power check, weak bits as
    /// stored). For measurement only, never for a store.
    pub fn peek(&self, addr: u32, buf: &mut [u8]) {
        let ss = self.geom.sector_size as usize;
        for (i, b) in buf.iter_mut().enumerate() {
            let a = addr as usize + i;
            *b = self.sectors[a / ss][a % ss];
        }
    }

    /// True when the sector reads all `0xFF` and has no weak bits.
    pub fn sector_is_blank(&self, sector: u32) -> bool {
        self.damage[sector as usize].weak.is_none()
            && self.sectors[sector as usize].iter().all(|&b| b == 0xFF)
    }

    /// Sectors that are not blank: the "sectors in use" measure.
    pub fn sectors_in_use(&self) -> u32 {
        (0..self.geom.sector_count)
            .filter(|&s| !self.sector_is_blank(s))
            .count() as u32
    }

    /// Program `data` at `addr` (clears bits: stored = stored & data). A
    /// program crossing a page boundary runs page by page, one operation each.
    pub fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), NorError> {
        if !self.powered {
            return Err(NorError::PowerLost);
        }
        self.check_range(addr, data.len())?;
        self.stats.program_calls += 1;
        let ps = self.geom.page_size as usize;
        let mut done = 0;
        while done < data.len() {
            let a = addr as usize + done;
            let n = (ps - a % ps).min(data.len() - done);
            self.program_page(a, &data[done..done + n])?;
            done += n;
        }
        Ok(())
    }

    /// Erase one sector to all `0xFF` (one operation).
    pub fn erase_sector(&mut self, sector: u32) -> Result<(), NorError> {
        if !self.powered {
            return Err(NorError::PowerLost);
        }
        if sector >= self.geom.sector_count {
            return Err(NorError::OutOfBounds);
        }
        let fate = self.begin_op();
        self.stats.erases_per_sector[sector as usize] += 1;
        match fate {
            OpFate::Run => {
                let cells = Arc::make_mut(&mut self.sectors[sector as usize]);
                cells.fill(0xFF);
                self.damage[sector as usize] = NorSectorState::default();
                Ok(())
            }
            OpFate::Tear => {
                self.tear_erase(sector as usize);
                Err(NorError::PowerLost)
            }
        }
    }

    fn program_page(&mut self, addr: usize, data: &[u8]) -> Result<(), NorError> {
        let ss = self.geom.sector_size as usize;
        let (sector, off) = (addr / ss, addr % ss);
        let fate = self.begin_op();
        self.stats.program_pages += 1;
        match fate {
            OpFate::Run => {
                self.stats.program_bytes += data.len() as u64;
                self.check_violations(sector, off, data);
                let cells = Arc::make_mut(&mut self.sectors[sector]);
                for (i, &d) in data.iter().enumerate() {
                    cells[off + i] &= d;
                }
                if let Some(weak) = &mut self.damage[sector].weak {
                    // A bit programmed to 0 is solid now; a 1 left alone stays weak.
                    for (i, &d) in data.iter().enumerate() {
                        weak[off + i] &= d;
                    }
                }
                Ok(())
            }
            OpFate::Tear => {
                self.tear_program(sector, off, data);
                Err(NorError::PowerLost)
            }
        }
    }

    /// Count (and in a pristine sector, maybe panic on) 0→1 programs.
    fn check_violations(&mut self, sector: usize, off: usize, data: &[u8]) {
        let cells = &self.sectors[sector];
        let weak = self.damage[sector].weak.as_deref();
        let mut bad = 0u64;
        for (i, &d) in data.iter().enumerate() {
            let w = weak.map(|w| w[off + i]).unwrap_or(0);
            if d & !cells[off + i] & !w != 0 {
                bad += 1;
            }
        }
        if bad > 0 {
            self.stats.violations_0_to_1 += bad;
            if self.panic_on_violation && self.damage[sector].is_pristine() {
                panic!(
                    "lp-nor-sim: 0->1 program at sector {sector} offset {off}: {bad} byte(s) \
                     ask a cleared bit to become 1 (real NOR cannot)"
                );
            }
        }
    }

    fn begin_op(&mut self) -> OpFate {
        if self.plan.cut_after == Some(self.ops_since_plan) {
            self.powered = false;
            self.ops_since_plan += 1;
            self.stats.ops_total += 1;
            self.stats.torn_ops += 1;
            return OpFate::Tear;
        }
        self.ops_since_plan += 1;
        self.stats.ops_total += 1;
        OpFate::Run
    }

    fn tear_program(&mut self, sector: usize, off: usize, data: &[u8]) {
        let tear = self.plan.tear;
        if tear == TearModel::Clean {
            return;
        }
        self.damage[sector].tainted = true;
        let rng = &mut self.tear_rng;
        let cells = Arc::make_mut(&mut self.sectors[sector]);
        match tear {
            TearModel::Clean => {}
            TearModel::BytePrefix => {
                let n = rng.below(data.len() as u64) as usize;
                for (i, &d) in data.iter().enumerate().take(n) {
                    cells[off + i] &= d;
                }
                let c = &mut cells[off + n];
                let clears = *c & !data[n];
                *c &= !(clears & rng.next_u8());
            }
            TearModel::RandomBits => {
                for (i, &d) in data.iter().enumerate() {
                    let c = &mut cells[off + i];
                    let clears = *c & !d;
                    *c &= !(clears & rng.next_u8());
                }
            }
        }
        // Bits a torn program did clear are solid; leave the weak map alone.
        if let Some(weak) = &mut self.damage[sector].weak {
            for i in 0..data.len() {
                weak[off + i] &= cells[off + i];
            }
        }
    }

    /// A torn erase, in one of three seeded shapes: (0) a byte-wise mix of
    /// old, `0xFF` and weak; (1) reads erased but carries a sprinkling of weak
    /// bits; (2) erased up to a point, old after it, weak around the edge.
    fn tear_erase(&mut self, sector: usize) {
        if self.plan.tear == TearModel::Clean {
            return;
        }
        let ss = self.geom.sector_size as usize;
        let rng = &mut self.tear_rng;
        let cells = Arc::make_mut(&mut self.sectors[sector]);
        let state = &mut self.damage[sector];
        state.tainted = true;
        let weak = state.weak_mask(ss);
        match rng.below(3) {
            0 => {
                for i in 0..ss {
                    match rng.below(10) {
                        0..=3 => {}
                        4..=7 => cells[i] = 0xFF,
                        _ => {
                            let m = rng.next_u8() | 1;
                            cells[i] |= m;
                            weak[i] |= m;
                        }
                    }
                }
            }
            1 => {
                cells.fill(0xFF);
                for w in weak.iter_mut() {
                    if rng.chance(1, 32) {
                        *w |= 1 << rng.below(8);
                    }
                }
            }
            _ => {
                let p = rng.below(ss as u64) as usize;
                cells[..p].fill(0xFF);
                let lo = p.saturating_sub(16);
                let hi = (p + 16).min(ss);
                for i in lo..hi {
                    let m = rng.next_u8();
                    cells[i] |= m;
                    weak[i] |= m;
                }
            }
        }
        if weak.iter().all(|&w| w == 0) {
            state.weak = None;
        }
    }

    fn check_range(&self, addr: u32, len: usize) -> Result<(), NorError> {
        if addr as u64 + len as u64 > self.geom.capacity() as u64 {
            return Err(NorError::OutOfBounds);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> NorFlashSim {
        NorFlashSim::new(NorGeometry::new(4, 4096, 256))
    }

    #[test]
    fn erase_sets_ones_and_program_clears_bits() {
        let mut f = NorFlashSim::filled(NorGeometry::new(4, 4096, 256), 0x00);
        f.erase_sector(1).unwrap();
        let mut b = [0u8; 4];
        f.read(4096, &mut b).unwrap();
        assert_eq!(b, [0xFF; 4]);
        f.program(4096, &[0xF0, 0x0F, 0xAA, 0xFF]).unwrap();
        f.program(4096, &[0x30, 0x0F, 0xAA, 0x00]).unwrap();
        f.read(4096, &mut b).unwrap();
        assert_eq!(b, [0x30, 0x0F, 0xAA, 0x00]);
        assert_eq!(f.stats().erases_per_sector[1], 1);
    }

    #[test]
    fn rewriting_same_bits_is_allowed() {
        let mut f = small();
        f.program(0, &[0x12]).unwrap();
        f.program(0, &[0x02]).unwrap();
        assert_eq!(f.stats().violations_0_to_1, 0);
    }

    #[test]
    #[should_panic(expected = "0->1 program")]
    fn zero_to_one_panics_in_a_pristine_sector() {
        let mut f = small();
        f.set_panic_on_violation(true);
        f.program(0, &[0x00]).unwrap();
        f.program(0, &[0x01]).unwrap();
    }

    #[test]
    fn zero_to_one_is_counted_when_not_panicking() {
        let mut f = small();
        f.set_panic_on_violation(false);
        f.program(0, &[0x00, 0x00]).unwrap();
        f.program(0, &[0x01, 0x80]).unwrap();
        assert_eq!(f.stats().violations_0_to_1, 2);
        let mut b = [0u8; 2];
        f.read(0, &mut b).unwrap();
        assert_eq!(b, [0, 0], "a cleared bit stays cleared");
    }

    #[test]
    fn multi_page_program_is_several_ops_and_a_cut_in_page_two_keeps_page_one() {
        let mut f = small();
        f.set_plan(FaultPlan::cut(1, TearModel::Clean, 7));
        let data = [0x00u8; 600]; // pages 0..256, 256..512, 512..600
        assert_eq!(f.program(0, &data), Err(NorError::PowerLost));
        assert_eq!(f.stats().program_pages, 2);
        let mut b = [0u8; 600];
        f.power_cycle(FaultPlan::none());
        f.read(0, &mut b).unwrap();
        assert!(b[..256].iter().all(|&x| x == 0));
        assert!(b[256..].iter().all(|&x| x == 0xFF));
    }

    #[test]
    fn unaligned_program_splits_at_page_boundaries() {
        let mut f = small();
        f.program(250, &[0u8; 10]).unwrap();
        assert_eq!(f.stats().program_pages, 2);
    }

    #[test]
    fn everything_after_the_cut_fails_until_power_cycle() {
        let mut f = small();
        f.set_plan(FaultPlan::cut(0, TearModel::Clean, 1));
        assert_eq!(f.erase_sector(0), Err(NorError::PowerLost));
        let mut b = [0u8; 1];
        assert_eq!(f.read(0, &mut b), Err(NorError::PowerLost));
        assert_eq!(f.program(0, &[0]), Err(NorError::PowerLost));
        assert_eq!(f.erase_sector(1), Err(NorError::PowerLost));
        f.power_cycle(FaultPlan::none());
        f.program(0, &[0]).unwrap();
        f.read(0, &mut b).unwrap();
        assert_eq!(b, [0]);
    }

    #[test]
    fn power_cycle_keeps_cells() {
        let mut f = small();
        f.program(10, &[0x42]).unwrap();
        f.set_plan(FaultPlan::cut(0, TearModel::Clean, 1));
        let _ = f.program(11, &[0x00]);
        f.power_cycle(FaultPlan::none());
        let mut b = [0u8; 2];
        f.read(10, &mut b).unwrap();
        assert_eq!(b, [0x42, 0xFF]);
    }

    #[test]
    fn clean_tear_does_nothing() {
        let mut f = small();
        f.set_plan(FaultPlan::cut(0, TearModel::Clean, 3));
        let _ = f.program(0, &[0u8; 64]);
        f.power_cycle(FaultPlan::none());
        let mut b = [0u8; 64];
        f.read(0, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 0xFF));
        assert!(f.sector_damage(0).is_pristine());
    }

    #[test]
    fn byte_prefix_tear_lands_a_prefix_then_a_partial_byte_then_nothing() {
        for seed in 0..50 {
            let mut f = small();
            f.set_plan(FaultPlan::cut(0, TearModel::BytePrefix, seed));
            let _ = f.program(0, &[0u8; 64]);
            f.power_cycle(FaultPlan::none());
            let mut b = [0u8; 64];
            f.read(0, &mut b).unwrap();
            let n = b.iter().position(|&x| x != 0).unwrap_or(64);
            assert!(n < 64, "the torn page is never complete");
            assert!(b[n + 1..].iter().all(|&x| x == 0xFF), "seed {seed}: {b:?}");
        }
    }

    #[test]
    fn random_bits_tear_clears_only_intended_bits() {
        let mut f = small();
        f.set_plan(FaultPlan::cut(0, TearModel::RandomBits, 9));
        let data: Vec<u8> = (0..256u32).map(|i| (i * 37) as u8).collect();
        let _ = f.program(0, &data);
        f.power_cycle(FaultPlan::none());
        let mut b = [0u8; 256];
        f.read(0, &mut b).unwrap();
        for (i, &x) in b.iter().enumerate() {
            assert_eq!(x & data[i], data[i] & x);
            assert_eq!(
                x | data[i],
                x,
                "only bits the program meant to clear are cleared"
            );
        }
        assert!(
            b.iter().zip(&data).any(|(x, d)| x != d),
            "some clears are missing"
        );
        assert!(b.iter().any(|&x| x != 0xFF), "some clears landed");
    }

    #[test]
    fn torn_erase_leaves_weak_bits_that_read_differently_until_a_full_erase() {
        let mut found = false;
        for seed in 0..20 {
            let mut f = NorFlashSim::filled(NorGeometry::new(2, 4096, 256), 0x00);
            f.set_plan(FaultPlan::cut(0, TearModel::RandomBits, seed));
            assert_eq!(f.erase_sector(0), Err(NorError::PowerLost));
            f.power_cycle(FaultPlan::none());
            if f.sector_damage(0).weak.is_none() {
                continue;
            }
            found = true;
            let mut a = vec![0u8; 4096];
            let mut b = vec![0u8; 4096];
            f.read(0, &mut a).unwrap();
            f.read(0, &mut b).unwrap();
            assert_ne!(a, b, "seed {seed}: weak bits read differently on two reads");
            f.erase_sector(0).unwrap();
            f.read(0, &mut a).unwrap();
            f.read(0, &mut b).unwrap();
            assert!(a.iter().all(|&x| x == 0xFF) && a == b);
            assert!(f.sector_damage(0).is_pristine());
        }
        assert!(found);
    }

    #[test]
    fn a_torn_erase_can_read_erased_and_still_be_weak() {
        let any = (0..60).any(|seed| {
            let mut f = NorFlashSim::filled(NorGeometry::new(1, 4096, 256), 0x00);
            f.set_plan(FaultPlan::cut(0, TearModel::BytePrefix, seed));
            let _ = f.erase_sector(0);
            f.power_cycle(FaultPlan::none());
            let mut cells = vec![0u8; 4096];
            f.peek(0, &mut cells);
            cells.iter().all(|&x| x == 0xFF) && f.sector_damage(0).weak_bits() > 0
        });
        assert!(any);
    }

    #[test]
    fn clean_model_never_tears_an_erase() {
        let mut f = NorFlashSim::filled(NorGeometry::new(1, 4096, 256), 0x00);
        f.set_plan(FaultPlan::cut(0, TearModel::Clean, 1));
        let _ = f.erase_sector(0);
        f.power_cycle(FaultPlan::none());
        let mut b = vec![0u8; 4096];
        f.read(0, &mut b).unwrap();
        assert!(b.iter().all(|&x| x == 0));
    }

    #[test]
    fn same_seed_same_result_and_clone_replays() {
        let run = |f: &mut NorFlashSim| {
            f.set_plan(FaultPlan::cut(5, TearModel::RandomBits, 1234));
            for i in 0..10u32 {
                if f.program(i * 300, &[0x11u8; 300]).is_err() {
                    break;
                }
            }
            f.power_cycle(FaultPlan::none());
            let mut b = vec![0u8; 4096];
            f.read(0, &mut b).unwrap();
            b
        };
        let base = small();
        let mut a = base.clone();
        let mut b = base.clone();
        assert_eq!(run(&mut a), run(&mut b));
        assert_eq!(a.stats(), b.stats());
    }

    #[test]
    fn out_of_bounds_is_an_error() {
        let mut f = small();
        let mut b = [0u8; 2];
        assert_eq!(f.read(4 * 4096 - 1, &mut b), Err(NorError::OutOfBounds));
        assert_eq!(f.erase_sector(4), Err(NorError::OutOfBounds));
    }

    #[test]
    fn read_watchdog_fires() {
        let mut f = small();
        f.set_read_budget(Some(2));
        let mut b = [0u8; 1];
        f.read(0, &mut b).unwrap();
        f.read(0, &mut b).unwrap();
        assert_eq!(f.read(0, &mut b), Err(NorError::Watchdog));
        f.power_cycle(FaultPlan::none());
        f.read(0, &mut b).unwrap();
    }

    #[test]
    fn sectors_in_use_counts_non_blank() {
        let mut f = small();
        assert_eq!(f.sectors_in_use(), 0);
        f.program(4096 * 2 + 5, &[0x00]).unwrap();
        assert_eq!(f.sectors_in_use(), 1);
    }
}
