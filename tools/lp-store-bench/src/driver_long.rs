//! Long histories (M3 P2): one store, mounted, walked for tens of thousands
//! of steps — the random driver's mix of pushes, re-pushes, saves, panel
//! writes and deletes over three slots — with a power cut every
//! `cut_every` steps and a full-state check (then a remount and the check
//! again) every `check_every`. Unlike the random driver, the store stays
//! mounted between cuts, so GC, the index's growth bound and the heads run
//! for thousands of steps on one mount, and the flash ages: at 128 sectors
//! a walk of 100k steps runs GC thousands of times.
//!
//! A cut step is a full `run_case` (cut, remount, old-or-new, re-run, one
//! more step, remount); the walk goes on from its flash. A step the store
//! refuses (`NoSpace`) is checked by `check_refusal`. The walk stops at its
//! first failure; its params replay it (`Reproducer::Long`).

use std::collections::BTreeMap;

use lp_nor_sim::{FaultPlan, NorFlashSim, SimRng, TearModel, WearMode, WearOut};
use serde::{Deserialize, Serialize};

use crate::cut_case::{CutCase, StepFixture, dry_run, probe_step, run_case};
use crate::driver_exhaustive::FailureRecord;
use crate::driver_random::{SLOTS, next_step};
use crate::gc_tally::GcTally;
use crate::oracle::{Failure, Model, apply_step, first_diff, read_state, run_step};
use crate::refusal_check::check_refusal;
use crate::workload::{board_step, push_step};
use crate::{
    Candidate, CandidateConfig, CandidateStore, CorpusSet, Reproducer, Scoreboard, StoreError,
    WorkloadKind, WorkloadSpec,
};

/// One long walk: replayable from these fields alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LongParams {
    pub candidate: String,
    pub config: CandidateConfig,
    pub corpora: Vec<String>,
    pub seed: u64,
    pub steps: u64,
    /// A cut every this many steps (0 = never).
    pub cut_every: u64,
    /// A full-state check, a remount and the check again, every this many
    /// steps (0 = only at the end).
    pub check_every: u64,
    /// Tear models a cut draws from, by name; empty = the three guessed ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tears: Vec<String>,
    /// Sectors that wear out (an injected failure; none by default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wear: Vec<WearSpec>,
    /// The edit mix: push each corpus into a slot (`a`, `b`, `c`, the
    /// corpora in order) once, then only saves, panel writes and the odd
    /// re-push — the live set stays near full, so garbage lands beside live
    /// records and GC has to copy. Off = the random driver's mix.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edit_mix: bool,
}

/// A [`WearOut`], as a reproducer carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WearSpec {
    pub sector: u32,
    pub after_erases: u64,
    /// `erase` (erases leave bits stuck at 0) or `program` (programs leave
    /// clears undone).
    pub erase: bool,
    pub seed: u64,
}

impl WearSpec {
    pub fn wear_out(&self) -> WearOut {
        WearOut {
            sector: self.sector,
            after_erases: self.after_erases,
            mode: if self.erase {
                WearMode::EraseFails
            } else {
                WearMode::ProgramFails
            },
            seed: self.seed,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LongSummary {
    pub driver: String,
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub seed: u64,
    pub steps_run: u64,
    pub steps_no_space: u64,
    pub cuts: u64,
    pub landed: u64,
    pub torn_erases: u64,
    pub checks: u64,
    pub failures: u64,
    pub non_atomic: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first_failure: Option<Failure>,
    /// GC over the walk's own mounts (a cut case's inner mounts are not
    /// counted: about four per cut, on the same flash).
    pub gc: GcTally,
    /// The cut case that failed first, if a cut did (with the walk's params
    /// and `long_walk_prefix`, enough to run it alone).
    pub failed_case: Option<CutCase>,
    pub logical_bytes: u64,
    pub error: Option<String>,
}

impl LongParams {
    pub fn tear_models(&self) -> Result<Vec<TearModel>, String> {
        if self.tears.is_empty() {
            return Ok(TearModel::ALL.to_vec());
        }
        self.tears
            .iter()
            .map(|t| TearModel::from_name(t).ok_or_else(|| format!("unknown tear model {t:?}")))
            .collect()
    }
}

pub fn long_walk(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
) -> LongSummary {
    let mut sum = LongSummary {
        driver: "long".into(),
        candidate: cand.name().into(),
        config: Some(p.config.clone()),
        seed: p.seed,
        ..Default::default()
    };
    match crate::catch_quiet(|| walk(cand, p, corpora, &mut sum, None)) {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => sum.error = Some(e),
        Err(panic) => fail(&mut sum, Failure::new("panic", panic)),
    }
    if let Some(f) = &sum.first_failure {
        sink.write(
            "failure",
            &FailureRecord {
                driver: "long".into(),
                failure: f.clone(),
                reproducer: Reproducer::Long(p.clone()),
            },
        );
    }
    sink.write("long_summary", &sum);
    sum
}

fn fail(sum: &mut LongSummary, f: Failure) {
    sum.failures += 1;
    *sum.kinds.entry(f.kind.clone()).or_default() += 1;
    if sum.first_failure.is_none() {
        sum.first_failure = Some(f);
    }
}

fn mount(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    mut flash: NorFlashSim,
    kind: &str,
) -> Result<Box<dyn CandidateStore>, Failure> {
    flash.power_cycle(FaultPlan::none());
    flash.set_read_budget(None);
    cand.mount(flash, cfg)
        .map_err(|(e, _)| Failure::new(kind, format!("mount: {e}")))
}

/// The walk `p` up to (not including) step `before`: the flash, the state it
/// holds and that step — to take a failing step apart (`long_walk` names the
/// step and its cut in the failure's detail and `failed_case`).
pub fn long_walk_prefix(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    before: u64,
) -> Result<(NorFlashSim, Model, crate::Step), String> {
    let mut sum = LongSummary::default();
    let out = walk(cand, p, corpora, &mut sum, Some(before))?;
    match (out, sum.first_failure) {
        (Some(v), _) => Ok(v),
        (None, Some(f)) => Err(format!("failed first: {}: {}", f.kind, f.detail)),
        (None, None) => Err(format!("the walk has no step {before}")),
    }
}

/// Run the walk; with `stop_before`, stop (unmounted) at that step and hand
/// back the flash, the model and the step.
fn walk(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    sum: &mut LongSummary,
    stop_before: Option<u64>,
) -> Result<Option<(NorFlashSim, Model, crate::Step)>, String> {
    let cfg = &p.config;
    let tears = p.tear_models()?;
    let mut rng = SimRng::new(p.seed ^ 0x10_C6_5EED);
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(p.wear.is_empty());
    for w in &p.wear {
        flash.add_wear_out(w.wear_out());
    }
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    let mut store = match mount(cand, cfg, flash, "unmountable") {
        Ok(s) => s,
        Err(f) => {
            fail(sum, f);
            return Ok(None);
        }
    };
    let mut model = Model::new();
    for i in 0..p.steps {
        let step = if i == 0 {
            board_step()
        } else if p.edit_mix {
            edit_step(&mut rng, &model, corpora, &p.corpora, i)?
        } else {
            next_step(&mut rng, &model, corpora, &p.corpora)?
        };
        if stop_before == Some(i) {
            return Ok(Some((sum.gc.unmount(store), model, step)));
        }
        sum.steps_run += 1;
        sum.logical_bytes += step.logical_bytes();
        let mut new = model.clone();
        apply_step(&mut new, &step);
        let r = if p.cut_every > 0 && i > 0 && i % p.cut_every == 0 {
            cut_step(cand, p, &tears, &mut rng, store, &model, &new, step, i, sum)
        } else {
            plain_step(cand, cfg, store, &model, &new, &step, sum)
        };
        (store, model) = match r {
            Ok(v) => v,
            Err(f) => {
                fail(sum, f);
                return Ok(None);
            }
        };
        if p.check_every > 0 && (i + 1) % p.check_every == 0 {
            store = match check(cand, cfg, store, &model, sum) {
                Ok(s) => s,
                Err(f) => {
                    fail(sum, f);
                    return Ok(None);
                }
            };
        }
    }
    match check(cand, cfg, store, &model, sum) {
        Ok(store) => drop(sum.gc.unmount(store)),
        Err(f) => fail(sum, f),
    }
    Ok(None)
}

type Walked = (Box<dyn CandidateStore>, Model);

/// The edit mix's step `i`: steps 1..=3 fill the slots, then the random
/// driver's saves, panel writes and re-pushes (its pushes and deletes
/// re-drawn while any slot holds a project).
fn edit_step(
    rng: &mut SimRng,
    model: &Model,
    corpora: &CorpusSet,
    names: &[String],
    i: u64,
) -> Result<crate::Step, String> {
    if let Some(slot) = SLOTS.get(i as usize - 1) {
        let c = corpora.get(&names[(i as usize - 1) % names.len()])?;
        let mut s = push_step(&format!("push-{slot}"), slot, &c, &BTreeMap::new());
        s.ops
            .insert(0, crate::Op::DeletePrefix(format!("/projects/{slot}/")));
        return Ok(s);
    }
    let any = model.keys().any(|p| p.starts_with("/projects/"));
    loop {
        let s = next_step(rng, model, corpora, names)?;
        if !any || !(s.label.starts_with("push-") || s.label == "delete") {
            return Ok(s);
        }
    }
}

/// A step on the mounted store; a refusal is checked after a remount.
fn plain_step(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    mut store: Box<dyn CandidateStore>,
    old: &Model,
    new: &Model,
    step: &crate::Step,
    sum: &mut LongSummary,
) -> Result<Walked, Failure> {
    match run_step(store.as_mut(), step) {
        Ok(()) => Ok((store, new.clone())),
        Err(StoreError::NoSpace) => {
            sum.steps_no_space += 1;
            check_refusal(cand, cfg, sum.gc.unmount(store), old, new)
        }
        Err(e) => Err(Failure::new("step_error_without_cut", e.to_string())),
    }
}

/// A step cut at a random op of it: the whole `run_case` drill.
#[allow(
    clippy::too_many_arguments,
    reason = "the walk's state, passed whole rather than bundled for one call site"
)]
fn cut_step(
    cand: &dyn Candidate,
    p: &LongParams,
    tears: &[TearModel],
    rng: &mut SimRng,
    store: Box<dyn CandidateStore>,
    old: &Model,
    new: &Model,
    step: crate::Step,
    i: u64,
    sum: &mut LongSummary,
) -> Result<Walked, Failure> {
    let cfg = &p.config;
    let pre = sum.gc.unmount(store);
    let (post, mount_ops, step_ops, err) = crate::catch_quiet(|| dry_run(cand, cfg, &pre, &step))
        .map_err(|e| Failure::new("panic", format!("dry run: {e}")))?
        .map_err(|e| Failure::new("unmountable", e))?;
    match err {
        None => {}
        Some(StoreError::NoSpace) => {
            sum.steps_no_space += 1;
            return check_refusal(cand, cfg, post, old, new);
        }
        Some(e) => return Err(Failure::new("step_error_without_cut", e.to_string())),
    }
    let next = probe_step(i);
    let mut after_next = new.clone();
    apply_step(&mut after_next, &next);
    let fx = StepFixture {
        index: i as usize,
        pre,
        old: old.clone(),
        new: new.clone(),
        step,
        next,
        after_next: after_next.clone(),
        mount_ops,
        step_ops,
        dry_error: None,
    };
    let tear = tears[rng.below(tears.len() as u64) as usize];
    let case = CutCase {
        candidate: cand.name().into(),
        config: cfg.clone(),
        workload: WorkloadSpec::new(WorkloadKind::Push, "long", p.seed),
        step: i as usize,
        cut_after: rng.below(step_ops + 1),
        tear: tear.name().into(),
        seed: rng.next_u64(),
        second: None,
    };
    let o = run_case(cand, cfg, &fx, &case);
    sum.cuts += 1;
    sum.landed += o.landed as u64;
    sum.torn_erases += o.torn_erase as u64;
    if let Some(mut f) = o.failure {
        f.detail = format!(
            "step {i} cut {}/{step_ops} {}: {}",
            case.cut_after, case.tear, f.detail
        );
        sum.failed_case.get_or_insert(case);
        return Err(f);
    }
    sum.non_atomic += !o.atomic as u64;
    let flash = o.final_flash.expect("a passing case returns its flash");
    Ok((mount(cand, cfg, flash, "unmountable")?, after_next))
}

/// The whole state against the model, then a remount and again.
fn check(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    mut store: Box<dyn CandidateStore>,
    model: &Model,
    sum: &mut LongSummary,
) -> Result<Box<dyn CandidateStore>, Failure> {
    sum.checks += 1;
    let paths = model.keys().cloned().collect();
    let state = read_state(store.as_mut(), &paths)?;
    if state != *model {
        return Err(Failure::new("check_wrong_state", first_diff(&state, model)));
    }
    let mut store = mount(cand, cfg, sum.gc.unmount(store), "remount_failed")?;
    let state = read_state(store.as_mut(), &paths)?;
    if state != *model {
        return Err(Failure::new(
            "check_remount_wrong_state",
            first_diff(&state, model),
        ));
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout, TreeStoreCandidate};

    fn params(sectors: u32) -> LongParams {
        LongParams {
            candidate: "t1".into(),
            config: CandidateConfig::new(sectors),
            corpora: vec!["syn:2:300".into(), "syn:4:500".into()],
            seed: 3,
            steps: 400,
            cut_every: 25,
            check_every: 100,
            tears: vec!["random_bits".into(), "calibrated".into()],
            wear: vec![],
            edit_mix: true,
        }
    }

    /// A short walk on a small flash: T1 runs GC, takes cuts and refusals,
    /// and passes; the in-place twin of the reference fails.
    #[test]
    fn a_short_walk_runs_gc_and_catches_the_broken_reference() {
        let set = CorpusSet::new(None);
        let sink = Scoreboard::memory();
        let s = long_walk(&TreeStoreCandidate, &params(12), &set, &sink);
        assert_eq!(s.failures, 0, "{s:?}");
        assert!(s.cuts >= 15 && s.checks >= 4, "{s:?}");
        assert!(s.gc.gc_runs.unwrap_or(0) > 20, "{s:?}");
        let bad = long_walk(
            &MemCandidate::new(MemLayout::InPlace),
            &LongParams {
                candidate: "mem-broken".into(),
                ..params(16)
            },
            &set,
            &sink,
        );
        assert!(bad.failures > 0, "{bad:?}");
        let rec = sink
            .records()
            .into_iter()
            .find(|r| r["type"] == "failure")
            .unwrap();
        let rec: FailureRecord = serde_json::from_value(rec).unwrap();
        let again = crate::replay(&rec.reproducer, &set).unwrap();
        assert_eq!(again.map(|f| f.kind), Some(rec.failure.kind));
    }
}
