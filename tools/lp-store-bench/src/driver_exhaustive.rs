//! Exhaustive single cut: for every swept step, every cut point, every tear
//! model and every seed, fork the pre-step flash, cut, judge. Parallel.

use std::collections::BTreeMap;
use std::sync::Mutex;

use lp_nor_sim::{SimRng, TearModel};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cut_case::{CaseOutcome, CutCase, SecondCut, StepFixture, prepare_fixtures, run_case};
use crate::oracle::Failure;
use crate::{Candidate, CandidateConfig, Scoreboard, Workload, WorkloadSpec};

/// What to sweep.
#[derive(Clone, Debug)]
pub struct SweepParams {
    pub tears: Vec<TearModel>,
    pub seeds: Vec<u64>,
    /// At most this many cut points per step, spread evenly (always 0 and n);
    /// `None` = every one.
    pub max_cuts_per_step: Option<u64>,
    /// Steps to sweep; `None` = from the workload's focus to its end.
    pub steps: Option<Vec<usize>>,
    /// Full failure records written per (sweep, tear); the rest are counted.
    pub failure_log_cap: usize,
}

impl Default for SweepParams {
    fn default() -> Self {
        Self {
            tears: TearModel::ALL.to_vec(),
            seeds: vec![1, 2],
            max_cuts_per_step: None,
            steps: None,
            failure_log_cap: 20,
        }
    }
}

/// One line of the scoreboard per (driver, candidate, config, workload, tear).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SweepSummary {
    pub driver: String,
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub workload: Option<WorkloadSpec>,
    pub tear: String,
    pub cases: u64,
    pub failures: u64,
    pub non_atomic: u64,
    pub landed: u64,
    pub kinds: BTreeMap<String, u64>,
    pub steps_swept: u64,
    pub steps_skipped: Vec<String>,
    pub max_cuts_per_step: Option<u64>,
    pub error: Option<String>,
}

/// A failed case, with what replays it (`lp-store-bench replay <file>`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailureRecord {
    pub driver: String,
    pub failure: Failure,
    pub reproducer: crate::Reproducer,
}

/// Cut points for a step of `n` operations.
pub fn cut_points(n: u64, max: Option<u64>) -> Vec<u64> {
    match max {
        Some(m) if n + 1 > m && m >= 2 => {
            let mut v: Vec<u64> = (0..m).map(|i| i * n / (m - 1)).collect();
            v.dedup();
            v
        }
        _ => (0..=n).collect(),
    }
}

/// A case's seed: the sweep seed mixed with where the cut is.
pub fn case_seed(base: u64, step: usize, k: u64) -> u64 {
    let mut r = SimRng::new(base ^ ((step as u64) << 40) ^ k.wrapping_mul(0x9E37_79B9));
    r.next_u64()
}

/// Run the exhaustive single-cut sweep; summaries (one per tear) are written to
/// `sink` and returned.
pub fn sweep_exhaustive(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    workload: &Workload,
    params: &SweepParams,
    sink: &Scoreboard,
) -> Vec<SweepSummary> {
    sweep_with(
        cand,
        cfg,
        workload,
        params,
        sink,
        "exhaustive",
        |fx, tear, seed| {
            cut_points(fx.step_ops, params.max_cuts_per_step)
                .into_iter()
                .map(|k| (k, case_seed(seed, fx.index, k), None))
                .map(|(k, s, second)| (k, s, second, tear))
                .collect()
        },
    )
}

type CaseGen<'a> = dyn Fn(&StepFixture, TearModel, u64) -> Vec<(u64, u64, Option<SecondCut>, TearModel)>
    + Sync
    + 'a;

/// The shared sweep loop: build fixtures, generate cases per step × tear ×
/// seed with `gen`, run them in parallel, aggregate, log failures.
pub fn sweep_with(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    workload: &Workload,
    params: &SweepParams,
    sink: &Scoreboard,
    driver: &str,
    generate: impl Fn(&StepFixture, TearModel, u64) -> Vec<(u64, u64, Option<SecondCut>, TearModel)>
    + Sync,
) -> Vec<SweepSummary> {
    let generate: &CaseGen = &generate;
    let blank = |tear: TearModel| SweepSummary {
        driver: driver.into(),
        candidate: cand.name().into(),
        config: Some(cfg.clone()),
        workload: Some(workload.spec.clone()),
        tear: tear.name().into(),
        max_cuts_per_step: params.max_cuts_per_step,
        ..Default::default()
    };
    let fixtures = match crate::catch_quiet(|| prepare_fixtures(cand, cfg, workload)) {
        Ok(Ok(f)) => f,
        Ok(Err(e)) | Err(e) => {
            let out: Vec<SweepSummary> = params
                .tears
                .iter()
                .map(|&t| SweepSummary {
                    error: Some(format!("fixtures: {e}")),
                    ..blank(t)
                })
                .collect();
            for s in &out {
                sink.write("sweep_summary", s);
            }
            return out;
        }
    };
    let steps: Vec<usize> = params
        .steps
        .clone()
        .unwrap_or_else(|| (workload.focus..workload.steps.len()).collect());
    let mut out = Vec::new();
    for &tear in &params.tears {
        let summary = Mutex::new(blank(tear));
        let logged = Mutex::new(0usize);
        for &si in &steps {
            let Some(fx) = fixtures.get(si) else {
                summary
                    .lock()
                    .unwrap()
                    .steps_skipped
                    .push(format!("{si}: not reached"));
                continue;
            };
            if let Some(e) = &fx.dry_error {
                summary
                    .lock()
                    .unwrap()
                    .steps_skipped
                    .push(format!("{si}: {e}"));
                continue;
            }
            summary.lock().unwrap().steps_swept += 1;
            let cases: Vec<CutCase> = params
                .seeds
                .iter()
                .flat_map(|&seed| generate(fx, tear, seed))
                .map(|(k, seed, second, tear)| CutCase {
                    candidate: cand.name().into(),
                    config: cfg.clone(),
                    workload: workload.spec.clone(),
                    step: si,
                    cut_after: k,
                    tear: tear.name().into(),
                    seed,
                    second,
                })
                .collect();
            cases.par_iter().for_each(|case| {
                let o: CaseOutcome = run_case(cand, cfg, fx, case);
                let mut s = summary.lock().unwrap();
                s.cases += 1;
                s.landed += o.landed as u64;
                if let Some(f) = &o.failure {
                    s.failures += 1;
                    *s.kinds.entry(f.kind.clone()).or_default() += 1;
                    drop(s);
                    let mut n = logged.lock().unwrap();
                    if *n < params.failure_log_cap {
                        *n += 1;
                        sink.write(
                            "failure",
                            &FailureRecord {
                                driver: driver.into(),
                                failure: f.clone(),
                                reproducer: crate::Reproducer::Case(case.clone()),
                            },
                        );
                    }
                } else if !o.atomic {
                    s.non_atomic += 1;
                }
            });
        }
        let s = summary.into_inner().unwrap();
        sink.write("sweep_summary", &s);
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};
    use crate::{CorpusSet, WorkloadKind};

    #[test]
    fn cut_points_cover_the_ends() {
        assert_eq!(cut_points(3, None), vec![0, 1, 2, 3]);
        let v = cut_points(1000, Some(5));
        assert_eq!(v, vec![0, 250, 500, 750, 1000]);
    }

    #[test]
    fn the_sweep_passes_a_correct_store_and_catches_the_broken_twin() {
        let set = CorpusSet::new(None);
        let wl = set
            .build(&WorkloadSpec::new(WorkloadKind::Repush, "syn:3:300", 1))
            .unwrap();
        let cfg = CandidateConfig::new(16);
        let params = SweepParams::default();
        let sink = Scoreboard::memory();
        let good = sweep_exhaustive(
            &MemCandidate::new(MemLayout::PingPong),
            &cfg,
            &wl,
            &params,
            &sink,
        );
        assert_eq!(good.len(), 3);
        assert!(
            good.iter().all(|s| s.failures == 0 && s.cases > 0),
            "{good:?}"
        );
        let bad = sweep_exhaustive(
            &MemCandidate::new(MemLayout::InPlace),
            &cfg,
            &wl,
            &params,
            &sink,
        );
        assert!(bad.iter().all(|s| s.failures > 0), "{bad:?}");
        let failures: Vec<_> = sink
            .records()
            .into_iter()
            .filter(|r| r["type"] == "failure")
            .collect();
        assert!(!failures.is_empty());
        // A reproducer replays to the same verdict.
        let rec: FailureRecord = serde_json::from_value(failures[0].clone()).unwrap();
        let replayed = crate::replay(&rec.reproducer, &set).unwrap();
        assert_eq!(replayed.as_ref().map(|f| &f.kind), Some(&rec.failure.kind));
    }
}
