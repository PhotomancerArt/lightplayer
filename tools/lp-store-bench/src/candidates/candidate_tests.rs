//! Shared unit-test bodies for the real candidates: a fault-free round trip
//! and a small exhaustive sweep on a synthetic corpus (fast enough for CI).

use std::collections::BTreeSet;

use lp_nor_sim::{FaultPlan, NorFlashSim};

use crate::driver_exhaustive::{SweepParams, SweepSummary, sweep_exhaustive};
use crate::oracle::{Model, apply_step, read_state, run_step};
use crate::{Candidate, CandidateConfig, CorpusSet, Scoreboard, WorkloadKind, WorkloadSpec};

/// Put, get, list, delete a prefix, commit, remount: the store gives back
/// exactly what the model says, across a power cycle.
pub fn round_trip(cand: &dyn Candidate) {
    let cfg = CandidateConfig::new(32);
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, &cfg).unwrap();
    let mut store = cand.mount(flash, &cfg).map_err(|(e, _)| e).unwrap();

    let wl = CorpusSet::new(None)
        .build(&WorkloadSpec::new(WorkloadKind::Repush, "syn:3:300", 1))
        .unwrap();
    let mut model = Model::new();
    for step in &wl.steps {
        run_step(store.as_mut(), step).unwrap();
        apply_step(&mut model, step);
    }
    store
        .put("/projects/a/.lp/panel.json", b"{\"k\": 1}\n")
        .unwrap();
    store.put("/projects/b/x.json", b"").unwrap();
    store.commit().unwrap();
    assert_eq!(
        store.get("/projects/a/.lp/panel.json").unwrap().as_deref(),
        Some(&b"{\"k\": 1}\n"[..])
    );
    assert_eq!(
        store.get("/projects/b/x.json").unwrap().as_deref(),
        Some(&[][..])
    );
    assert_eq!(store.get("/projects/a/nope.json").unwrap(), None);
    let a = store.list("/projects/a/").unwrap();
    assert!(a.iter().all(|p| p.starts_with("/projects/a/")));
    assert!(a.contains(&"/projects/a/.lp/panel.json".to_string()));
    let mut sorted = a.clone();
    sorted.sort();
    assert_eq!(a, sorted);

    store.delete_prefix("/projects/b/").unwrap();
    store.commit().unwrap();
    assert_eq!(store.get("/projects/b/x.json").unwrap(), None);
    model.insert(
        "/projects/a/.lp/panel.json".into(),
        std::sync::Arc::new(b"{\"k\": 1}\n".to_vec()),
    );

    let mut flash = store.into_flash();
    flash.power_cycle(FaultPlan::none());
    let mut store = cand.mount(flash, &cfg).map_err(|(e, _)| e).unwrap();
    let paths: BTreeSet<String> = model.keys().cloned().collect();
    let state = read_state(store.as_mut(), &paths).unwrap();
    assert_eq!(state, model);
    assert!(store.report().ram_bytes > 0);
}

/// A capped exhaustive sweep of two save steps on a synthetic corpus, every
/// tear model, one seed; the per-tear summaries summed.
pub fn small_sweep(cand: &dyn Candidate) -> SweepSummary {
    let cfg = CandidateConfig::new(32);
    let wl = CorpusSet::new(None)
        .build(&WorkloadSpec::new(WorkloadKind::Save, "syn:3:300", 1))
        .unwrap();
    let params = SweepParams {
        seeds: vec![1],
        max_cuts_per_step: Some(24),
        steps: Some(vec![2, 3]),
        ..Default::default()
    };
    let sums = sweep_exhaustive(cand, &cfg, &wl, &params, &Scoreboard::memory());
    let mut total = SweepSummary::default();
    for s in sums {
        assert_eq!(s.error, None, "{s:?}");
        total.cases += s.cases;
        total.failures += s.failures;
        total.non_atomic += s.non_atomic;
        total.landed += s.landed;
        for (k, n) in s.kinds {
            *total.kinds.entry(k).or_default() += n;
        }
    }
    total
}
