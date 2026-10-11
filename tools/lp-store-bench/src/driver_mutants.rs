//! Mutation testing (M3 P4): the oracle's power. For each of T1's
//! deliberately broken stores (`lp_tree_store::mutants`, feature `mutants`),
//! run one fixed set of drivers and count what they catch; the unmutated
//! store runs the same set first and must fail nothing. A mutant no driver
//! catches marks a place the tests are blind (or a guarantee another one
//! already covers — the report says which).
//!
//! The set, in order (the order "caught first" is read in): exhaustive cuts
//! of small workloads under the guessed tears and `calibrated`; the same at
//! a partition where GC runs; double cuts; long walks on a small flash; a
//! full-flash run; a mount fuzz; and a long walk with two sectors wearing
//! out. Everything is synthetic (no corpus needed) and runs in seconds per
//! mutant (release).

use std::collections::BTreeMap;

use lp_nor_sim::TearModel;
use lp_tree_store::mutants::{Mutant, active_mutant, set_mutant};
use serde::{Deserialize, Serialize};

use crate::candidates::TreeStoreCandidate;
use crate::driver_double_cut::{DoubleCutParams, sweep_double_cut};
use crate::driver_exhaustive::{SweepParams, SweepSummary, sweep_exhaustive};
use crate::driver_full_flash::{FullFlashParams, full_flash};
use crate::driver_fuzz::{FuzzParams, fuzz};
use crate::driver_long::{LongParams, WearSpec, long_walk};
use crate::{CandidateConfig, CorpusSet, Scoreboard, WorkloadSpec};

/// One driver's tally for one mutant.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverTally {
    pub driver: String,
    pub cases: u64,
    pub failures: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first: Option<String>,
    /// Failure kinds the unmutated store does not show in this driver.
    #[serde(default)]
    pub new_kinds: BTreeMap<String, u64>,
}

/// A mutant's line in the table (`mutant` = `none` for the unmutated store).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MutantResult {
    pub mutant: String,
    pub drivers: Vec<DriverTally>,
    pub failures: u64,
    /// The first driver in the set's order with a failure kind the unmutated
    /// store does not show there (for the unmutated store: the first driver
    /// with any failure).
    pub caught_by: Option<String>,
}

/// Every mutant (or those named), the unmutated store first.
pub fn run_mutants(only: &[String], sink: &Scoreboard) -> Result<Vec<MutantResult>, String> {
    let mut list: Vec<Option<Mutant>> = vec![None];
    for m in Mutant::ALL {
        if only.is_empty() || only.iter().any(|o| o == m.name()) {
            list.push(Some(m));
        }
    }
    for o in only {
        if o != "none" && Mutant::from_name(o).is_none() {
            return Err(format!("unknown mutant {o:?}"));
        }
    }
    let mut out: Vec<MutantResult> = Vec::new();
    for m in list {
        set_mutant(m);
        let mut r = run_set(m.map_or("none", |m| m.name()));
        set_mutant(None);
        if let Some(base) = out.first() {
            judge_against(&mut r, base);
        }
        sink.write("mutant_result", &r);
        out.push(r);
    }
    debug_assert!(active_mutant().is_none());
    Ok(out)
}

fn run_set(name: &str) -> MutantResult {
    let corpora = CorpusSet::new(None);
    let sink = Scoreboard::memory();
    let cand = TreeStoreCandidate;
    let tears = vec![
        TearModel::Clean,
        TearModel::BytePrefix,
        TearModel::RandomBits,
        TearModel::Calibrated,
    ];
    let tear_names: Vec<String> = tears.iter().map(|t| t.name().to_string()).collect();
    let mut drivers = Vec::new();

    // 1. Exhaustive (sampled) single cuts.
    let sweeps = |sectors: u32, specs: &[(&str, Option<Vec<usize>>)], label: &str| {
        let mut t = DriverTally {
            driver: label.into(),
            ..Default::default()
        };
        for (spec, steps) in specs {
            let wl = corpora
                .build(&WorkloadSpec::parse(spec).expect("spec"))
                .expect("workload");
            let p = SweepParams {
                tears: tears.clone(),
                seeds: vec![1],
                max_cuts_per_step: Some(48),
                steps: steps.clone(),
                ..Default::default()
            };
            add_sweeps(
                &mut t,
                &sweep_exhaustive(&cand, &CandidateConfig::new(sectors), &wl, &p, &sink),
            );
        }
        t
    };
    drivers.push(sweeps(
        24,
        &[
            ("push:syn:6:800", None),
            ("repush:syn:4:500", None),
            ("save:syn:4:500", Some(vec![2, 3, 4])),
            ("panel:syn:4:500", Some(vec![2, 3, 4])),
            ("switch:syn:3:400,syn:5:300", None),
        ],
        "sweep",
    ));
    // 2. The same where GC runs (save and panel near full).
    drivers.push(sweeps(
        10,
        &[
            ("save:syn:5:900", Some(vec![2, 6, 10, 14, 18])),
            ("panel:syn:5:900", Some(vec![2, 30, 60, 90])),
        ],
        "sweep_gc",
    ));
    // 3. Double cuts.
    {
        let mut t = DriverTally {
            driver: "double".into(),
            ..Default::default()
        };
        for (spec, sectors) in [("save:syn:4:500", 24), ("save:syn:5:900", 10)] {
            let wl = corpora.build(&WorkloadSpec::parse(spec).unwrap()).unwrap();
            let p = SweepParams {
                tears: tears.clone(),
                seeds: vec![1],
                steps: Some(vec![2, 3]),
                ..Default::default()
            };
            add_sweeps(
                &mut t,
                &sweep_double_cut(
                    &cand,
                    &CandidateConfig::new(sectors),
                    &wl,
                    &p,
                    &DoubleCutParams {
                        first_cuts: 6,
                        mount_cuts: 12,
                        rerun_cuts: 6,
                    },
                    &sink,
                ),
            );
        }
        drivers.push(t);
    }
    // 4. Long walks on a small flash.
    let long = |label: &str, wear: Vec<WearSpec>, sectors: u32, seeds: &[u64]| {
        let mut t = DriverTally {
            driver: label.into(),
            ..Default::default()
        };
        for &seed in seeds {
            let s = long_walk(
                &cand,
                &LongParams {
                    candidate: "t1".into(),
                    config: CandidateConfig::new(sectors),
                    corpora: vec!["syn:3:1200".into(), "syn:4:700".into(), "syn:2:2500".into()],
                    seed,
                    steps: 1500,
                    cut_every: 7,
                    check_every: 50,
                    tears: tear_names.clone(),
                    wear: wear.clone(),
                    edit_mix: seed % 2 == 1,
                    piece_steps: 0,
                },
                &corpora,
                &sink,
            );
            t.cases += s.cuts + s.checks;
            add_failure(&mut t, s.failures, &s.kinds, s.first_failure.as_ref());
            if let Some(e) = s.error {
                add_failure(&mut t, 1, &BTreeMap::from([("error".into(), 1)]), None);
                t.first.get_or_insert(e);
            }
        }
        t
    };
    drivers.push(long("long", vec![], 12, &[1, 2, 3, 4]));
    // 5. Full flash.
    {
        let mut t = DriverTally {
            driver: "full_flash".into(),
            ..Default::default()
        };
        for seed in [1, 2] {
            let s = full_flash(
                &cand,
                &FullFlashParams {
                    candidate: "t1".into(),
                    config: CandidateConfig::new(16),
                    corpus: "syn:3:900".into(),
                    seed,
                    edge_steps: 24,
                    cuts_per_step: 10,
                    tears: tear_names.clone(),
                },
                &corpora,
                &sink,
            );
            t.cases += s.cases;
            add_failure(&mut t, s.failures, &s.kinds, s.first_failure.as_ref());
            if let Some(e) = s.error {
                add_failure(&mut t, 1, &BTreeMap::from([("error".into(), 1)]), None);
                t.first.get_or_insert(e);
            }
        }
        drivers.push(t);
    }
    // 6. Mount fuzz.
    {
        let s = fuzz(
            &cand,
            &FuzzParams {
                candidate: "t1".into(),
                config: CandidateConfig::new(16),
                corpora: vec!["syn:3:600".into(), "syn:4:900".into()],
                seed: 11,
                cases: 600,
                histories: 4,
                history_steps: 14,
                tears: tear_names.clone(),
                only_case: None,
            },
            &corpora,
            &sink,
        );
        let mut t = DriverTally {
            driver: "fuzz".into(),
            cases: s.cases,
            ..Default::default()
        };
        add_failure(&mut t, s.failures, &s.kinds, s.first_failure.as_ref());
        drivers.push(t);
    }
    // 7. Wear: two sectors wear out (one on erases, one on programs).
    drivers.push(long(
        "wear",
        vec![
            WearSpec {
                sector: 3,
                after_erases: 4,
                erase: true,
                seed: 5,
            },
            WearSpec {
                sector: 8,
                after_erases: 4,
                erase: false,
                seed: 6,
            },
        ],
        16,
        &[1, 2],
    ));

    let failures = drivers.iter().map(|d| d.failures).sum();
    let caught_by = drivers
        .iter()
        .find(|d| d.failures > 0)
        .map(|d| d.driver.clone());
    MutantResult {
        mutant: name.into(),
        drivers,
        failures,
        caught_by,
    }
}

/// A driver catches a mutant with a failure kind the unmutated store does
/// not show in that driver (so a driver the unmutated store fails in — the
/// open defect at the edge — cannot claim a catch with that same failure).
fn judge_against(r: &mut MutantResult, base: &MutantResult) {
    for d in &mut r.drivers {
        let known = base
            .drivers
            .iter()
            .find(|b| b.driver == d.driver)
            .map(|b| b.kinds.clone())
            .unwrap_or_default();
        d.new_kinds = d
            .kinds
            .iter()
            .filter(|(k, _)| !known.contains_key(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
    }
    r.caught_by = r
        .drivers
        .iter()
        .find(|d| !d.new_kinds.is_empty())
        .map(|d| d.driver.clone());
}

fn add_sweeps(t: &mut DriverTally, out: &[SweepSummary]) {
    for s in out {
        t.cases += s.cases;
        add_failure(t, s.failures, &s.kinds, None);
        if let Some(e) = &s.error {
            add_failure(t, 1, &BTreeMap::from([("error".into(), 1)]), None);
            t.first.get_or_insert(e.clone());
        }
    }
}

fn add_failure(
    t: &mut DriverTally,
    n: u64,
    kinds: &BTreeMap<String, u64>,
    first: Option<&crate::oracle::Failure>,
) {
    t.failures += n;
    for (k, v) in kinds {
        *t.kinds.entry(k.clone()).or_default() += v;
    }
    if t.first.is_none() {
        if let Some(f) = first {
            t.first = Some(format!("{}: {}", f.kind, f.detail));
        } else if let Some((k, _)) = kinds.iter().next() {
            t.first = Some(k.clone());
        }
    }
}
