//! Fault-free measures: space, peak, mount cost, write amplification, erase
//! spread, RAM — and the smallest partition a workload fits in.

use serde::{Deserialize, Serialize};

use crate::candidate_report::CandidateReport;
use crate::oracle::run_step;
use crate::{Candidate, CandidateConfig, Workload, WorkloadSpec};
use lp_nor_sim::{FaultPlan, NorFlashSim};

/// What one fault-free run of a workload cost.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeasureResult {
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub workload: Option<WorkloadSpec>,
    pub ok: bool,
    pub error: Option<String>,
    pub failed_step: Option<usize>,
    pub steps: usize,
    /// Bytes the workload asked the store to hold, summed over every put.
    pub logical_bytes: u64,
    pub program_bytes: u64,
    pub write_amp: f64,
    pub erases_total: u64,
    pub erases_min: u32,
    pub erases_median: u32,
    pub erases_max: u32,
    /// Flash sectors not blank at the end / at the most during the run.
    pub sectors_nonblank_end: u32,
    pub sectors_nonblank_peak: u32,
    /// The store's own count (littlefs `fs_size`, T1 live sectors, …).
    pub used_sectors_end: Option<u32>,
    pub used_sectors_max: Option<u32>,
    /// A fresh mount of the final flash.
    pub mount_read_bytes: u64,
    pub mount_read_calls: u64,
    pub mount_ops: u64,
    pub report: Option<CandidateReport>,
    pub violations_0_to_1: u64,
    /// Logical bytes of the store's live content at the end.
    pub live_logical_bytes: u64,
}

/// Format, mount once, run every step in that one mount, then measure a fresh
/// mount of the result.
pub fn measure(cand: &dyn Candidate, cfg: &CandidateConfig, wl: &Workload) -> MeasureResult {
    let mut r = MeasureResult {
        candidate: cand.name().into(),
        config: Some(cfg.clone()),
        workload: Some(wl.spec.clone()),
        steps: wl.steps.len(),
        ..Default::default()
    };
    match crate::catch_quiet(|| measure_inner(cand, cfg, wl, &mut r)) {
        Ok(Ok(())) => r.ok = r.error.is_none(),
        Ok(Err(e)) => r.error = Some(e),
        Err(p) => r.error = Some(format!("panic: {p}")),
    }
    r
}

fn measure_inner(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    wl: &Workload,
    r: &mut MeasureResult,
) -> Result<(), String> {
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    flash.reset_stats();
    flash.reset_peak();
    let mut store = cand
        .mount(flash, cfg)
        .map_err(|(e, _)| format!("mount: {e}"))?;
    let mut model = crate::oracle::Model::new();
    for (i, step) in wl.steps.iter().enumerate() {
        r.logical_bytes += step.logical_bytes();
        if let Err(e) = run_step(store.as_mut(), step) {
            r.error = Some(e.to_string());
            r.failed_step = Some(i);
            break;
        }
        crate::oracle::apply_step(&mut model, step);
        if let Some(u) = store.report().used_sectors {
            r.used_sectors_max = Some(r.used_sectors_max.unwrap_or(0).max(u));
        }
    }
    r.used_sectors_end = store.report().used_sectors;
    let flash = store.into_flash();
    let st = flash.stats();
    r.program_bytes = st.program_bytes;
    r.write_amp = if r.logical_bytes > 0 {
        st.program_bytes as f64 / r.logical_bytes as f64
    } else {
        0.0
    };
    r.violations_0_to_1 = st.violations_0_to_1;
    let mut e = st.erases_per_sector.clone();
    e.sort_unstable();
    r.erases_total = st.erases_total();
    r.erases_min = e.first().copied().unwrap_or(0);
    r.erases_max = e.last().copied().unwrap_or(0);
    r.erases_median = e.get(e.len() / 2).copied().unwrap_or(0);
    r.sectors_nonblank_end = flash.sectors_in_use();
    r.sectors_nonblank_peak = flash.peak_sectors_in_use();
    r.live_logical_bytes = model.values().map(|b| b.len() as u64).sum();
    let mut cold = flash;
    cold.power_cycle(FaultPlan::none());
    cold.reset_stats();
    let store = cand
        .mount(cold, cfg)
        .map_err(|(e, _)| format!("remount: {e}"))?;
    let st = store.flash_snapshot().stats().clone();
    r.mount_read_bytes = st.read_bytes;
    r.mount_read_calls = st.read_calls;
    r.mount_ops = st.ops_total;
    r.report = Some(store.report());
    Ok(())
}

/// The smallest sector count in `lo..=hi` at which `wl` completes fault-free
/// (binary search; assumes a store that fits in S sectors fits in S + 1).
pub fn min_sectors(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    wl: &Workload,
    lo: u32,
    hi: u32,
) -> Option<u32> {
    let fits = |s: u32| {
        let mut c = cfg.clone();
        c.sectors = s;
        measure(cand, &c, wl).ok
    };
    if !fits(hi) {
        return None;
    }
    let (mut lo, mut hi) = (lo, hi);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};
    use crate::{CorpusSet, WorkloadKind};

    #[test]
    fn measure_counts_and_min_sectors_searches() {
        let wl = CorpusSet::new(None)
            .build(&WorkloadSpec::new(WorkloadKind::Save, "syn:3:300", 1))
            .unwrap();
        let cand = MemCandidate::new(MemLayout::PingPong);
        let m = measure(&cand, &CandidateConfig::new(16), &wl);
        assert!(m.ok, "{m:?}");
        assert!(m.write_amp > 1.0 && m.mount_read_bytes > 0);
        let min = min_sectors(&cand, &CandidateConfig::new(16), &wl, 2, 64).unwrap();
        assert!((2..=4).contains(&min), "{min}");
    }
}
