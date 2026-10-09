//! T1's GC dials where GC runs (M3 P2): `gc_policy` × `reserve` × codec at
//! partitions just above c40's smallest (the race's dial sweep ran at 96–176
//! sectors, where c40 never meets GC). Each row: the c40 save and panel
//! workloads fault-free (GC runs, copies, write amplification, erase
//! spread) and a reduced cut sweep of the save workload (cases, failures).
//!
//! Partitions: `host_deflate` at 28, 36 and 48 sectors (c40's smallest
//! there is 23–26); `stored` at 72, 80 and 96 (its smallest is 57–70: c40
//! stored does not fit at 28–48 at all).

use std::collections::BTreeMap;

use lp_nor_sim::TearModel;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::candidates::TreeStoreCandidate;
use crate::driver_exhaustive::{SweepParams, sweep_exhaustive};
use crate::driver_measure::measure;
use crate::{CandidateConfig, CorpusSet, Scoreboard, WorkloadKind, WorkloadSpec};

/// One setting's results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GcDialRow {
    pub candidate: String,
    pub config: CandidateConfig,
    pub workload: String,
    pub ok: bool,
    pub error: Option<String>,
    pub gc_runs: u64,
    pub gc_copies: u64,
    pub write_amp: f64,
    pub erases_median: u32,
    pub erases_max: u32,
    pub cut_cases: u64,
    pub cut_failures: u64,
    pub cut_kinds: BTreeMap<String, u64>,
}

/// The settings swept (`quick`: one partition per codec, two reserves).
pub fn gc_dial_settings(quick: bool) -> Vec<CandidateConfig> {
    let mut out = Vec::new();
    let reserves: &[&str] = if quick { &["2", "3"] } else { &["2", "3", "4"] };
    for codec in ["host_deflate", "stored"] {
        let parts: &[u32] = match (codec, quick) {
            ("host_deflate", false) => &[28, 36, 48],
            ("host_deflate", true) => &[28],
            (_, false) => &[72, 80, 96],
            (_, true) => &[72],
        };
        for &sectors in parts {
            for policy in ["greedy", "cost_benefit"] {
                for r in reserves {
                    out.push(
                        CandidateConfig::new(sectors)
                            .with_dial("codec", codec)
                            .with_dial("gc_policy", policy)
                            .with_dial("reserve", r),
                    );
                }
            }
        }
    }
    out
}

/// Run every setting (in parallel), each row written to `sink` as
/// `gc_dial_row`.
pub fn gc_dial_sweep(
    corpora: &CorpusSet,
    sink: &Scoreboard,
    settings: &[CandidateConfig],
    tears: &[TearModel],
    cuts_per_step: u64,
) -> Vec<GcDialRow> {
    settings
        .par_iter()
        .flat_map_iter(|cfg| {
            let mut rows = Vec::new();
            for kind in [WorkloadKind::Save, WorkloadKind::Panel] {
                let spec = WorkloadSpec::new(kind, "c40", 1);
                let Ok(wl) = corpora.build(&spec) else {
                    continue;
                };
                let m = measure(&TreeStoreCandidate, cfg, &wl);
                let extra = |k: &str| {
                    m.run_report
                        .as_ref()
                        .and_then(|r| r.extra.get(k))
                        .copied()
                        .unwrap_or(0.0) as u64
                };
                let mut row = GcDialRow {
                    candidate: "t1".into(),
                    config: cfg.clone(),
                    workload: spec.label(),
                    ok: m.ok,
                    error: m.error.clone(),
                    gc_runs: extra("gc_runs"),
                    gc_copies: extra("gc_copies"),
                    write_amp: m.write_amp,
                    erases_median: m.erases_median,
                    erases_max: m.erases_max,
                    cut_cases: 0,
                    cut_failures: 0,
                    cut_kinds: BTreeMap::new(),
                };
                if kind == WorkloadKind::Save {
                    let p = SweepParams {
                        tears: tears.to_vec(),
                        seeds: vec![1],
                        max_cuts_per_step: Some(cuts_per_step),
                        steps: Some(vec![2, 8, 14, 21]),
                        ..Default::default()
                    };
                    for s in sweep_exhaustive(&TreeStoreCandidate, cfg, &wl, &p, sink) {
                        row.cut_cases += s.cases;
                        row.cut_failures += s.failures;
                        for (k, v) in s.kinds {
                            *row.cut_kinds.entry(k).or_default() += v;
                        }
                    }
                }
                sink.write("gc_dial_row", &row);
                rows.push(row);
            }
            rows
        })
        .collect()
}
