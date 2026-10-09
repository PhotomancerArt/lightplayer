//! Counting a store's garbage collection across mounts: a candidate's
//! self-report (`CandidateReport.extra`) covers one mount, so a driver that
//! remounts adds each mount's figures up before it lets the store go.

use serde::{Deserialize, Serialize};

use crate::{CandidateReport, CandidateStore};

/// GC figures summed over the mounts a driver held (T1 reports them; the
/// littlefs and sequential-storage candidates do not, so they stay `None`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GcTally {
    /// Victims collected.
    pub gc_runs: Option<u64>,
    /// Records copied out of victims.
    pub gc_copies: Option<u64>,
    /// Sectors retired after a failed read-back.
    pub retired_sectors: Option<u64>,
    /// Erases the flash has taken (its counters carry across clones and
    /// power cycles, so the latest unmount's figure is the total), and the
    /// most on one sector.
    #[serde(default)]
    pub erases_total: u64,
    #[serde(default)]
    pub erases_max: u32,
}

impl GcTally {
    /// Add one mount's report.
    pub fn add(&mut self, r: &CandidateReport) {
        let add = |slot: &mut Option<u64>, key: &str| {
            if let Some(v) = r.extra.get(key) {
                *slot = Some(slot.unwrap_or(0) + *v as u64);
            }
        };
        add(&mut self.gc_runs, "gc_runs");
        add(&mut self.gc_copies, "gc_copies");
        if let Some(v) = r.extra.get("retired_sectors") {
            // Retirement persists: the newest mount's count is the total.
            self.retired_sectors = Some(self.retired_sectors.unwrap_or(0).max(*v as u64));
        }
    }

    /// Unmount `store` (its report added first) and hand the flash back.
    pub fn unmount(&mut self, store: Box<dyn CandidateStore>) -> lp_nor_sim::NorFlashSim {
        self.add(&store.report());
        let flash = store.into_flash();
        self.erases_total = flash.stats().erases_total();
        self.erases_max = flash
            .stats()
            .erases_per_sector
            .iter()
            .copied()
            .max()
            .unwrap_or(0);
        flash
    }

    pub fn merge(&mut self, o: &GcTally) {
        for (a, b) in [
            (&mut self.gc_runs, o.gc_runs),
            (&mut self.gc_copies, o.gc_copies),
        ] {
            if let Some(b) = b {
                *a = Some(a.unwrap_or(0) + b);
            }
        }
        if let Some(b) = o.retired_sectors {
            self.retired_sectors = Some(self.retired_sectors.unwrap_or(0).max(b));
        }
        self.erases_total += o.erases_total;
        self.erases_max = self.erases_max.max(o.erases_max);
    }
}
