//! Double cuts: a first cut at a sampled point of the step, then a second cut
//! during the mount (and any repair) that follows it, or during the re-run of
//! the interrupted step. Judged after the final clean mount.

use lp_nor_sim::TearModel;

use crate::cut_case::{SecondCut, mount_ops_after_cut};
use crate::driver_exhaustive::{SweepParams, SweepSummary, case_seed, cut_points, sweep_with};
use crate::{Candidate, CandidateConfig, Scoreboard, Workload};

/// How densely to sample.
#[derive(Clone, Debug)]
pub struct DoubleCutParams {
    /// First-cut points per step.
    pub first_cuts: u64,
    /// Second-cut points in the mount after the first cut (all of them when
    /// the mount writes fewer).
    pub mount_cuts: u64,
    /// Second-cut points in the re-run.
    pub rerun_cuts: u64,
}

impl Default for DoubleCutParams {
    fn default() -> Self {
        Self {
            first_cuts: 8,
            mount_cuts: 32,
            rerun_cuts: 8,
        }
    }
}

pub fn sweep_double_cut(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    workload: &Workload,
    params: &SweepParams,
    dc: &DoubleCutParams,
    sink: &Scoreboard,
) -> Vec<SweepSummary> {
    sweep_with(
        cand,
        cfg,
        workload,
        params,
        sink,
        "double_cut",
        |fx, tear, seed| {
            let mut out = Vec::new();
            // A clean first cut leaves nothing for the mount to repair; still useful.
            for k in cut_points(fx.step_ops.saturating_sub(1), Some(dc.first_cuts)) {
                let s = case_seed(seed, fx.index, k);
                if let Some(m2) = mount_ops_after_cut(cand, cfg, fx, k, tear, s)
                    && m2 > 0
                {
                    for j in cut_points(m2 - 1, Some(dc.mount_cuts)) {
                        out.push((k, s, Some(SecondCut::Mount { at: j }), tear));
                    }
                }
                for j in cut_points(fx.step_ops, Some(dc.rerun_cuts)) {
                    out.push((k, s, Some(SecondCut::Rerun { at: j }), tear));
                }
            }
            out
        },
    )
}

/// The tear models worth a double cut (all of them).
pub fn double_cut_tears() -> Vec<TearModel> {
    TearModel::ALL.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};
    use crate::{CorpusSet, WorkloadKind, WorkloadSpec};

    #[test]
    fn double_cuts_pass_the_reference_store() {
        let wl = CorpusSet::new(None)
            .build(&WorkloadSpec::new(WorkloadKind::Save, "syn:2:200", 1))
            .unwrap();
        let params = SweepParams {
            steps: Some(vec![2, 3]),
            seeds: vec![1],
            ..Default::default()
        };
        let out = sweep_double_cut(
            &MemCandidate::new(MemLayout::PingPong),
            &CandidateConfig::new(16),
            &wl,
            &params,
            &DoubleCutParams::default(),
            &Scoreboard::memory(),
        );
        assert!(
            out.iter().all(|s| s.failures == 0 && s.cases > 0),
            "{out:?}"
        );
    }
}
