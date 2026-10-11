//! Full flash (M3 P2): fill the store with copies of a project until it
//! refuses (`NoSpace`), then work at the edge — saves, panel writes,
//! re-pushes, a delete, a push that cannot fit — with cuts in every step, and
//! finally free space and prove the store takes writes again.
//!
//! The checks: a refused step leaves every path old or new and, in a
//! step-atomic store, exactly the old state (`check_refusal`); a cut inside a
//! refused step leaves the old state too; a cut inside a step that fits is a
//! full `run_case` (old or new, re-run to new, remount) with an empty next
//! step (at the edge a "one more file" probe is not a promise any store can
//! make); and after the edge, deleting every copy but one and pushing a new
//! copy must succeed (`no_recovery` otherwise).

use std::collections::BTreeMap;
use std::sync::Arc;

use lp_nor_sim::{FaultPlan, NorFlashSim, SimRng, TearModel};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cut_case::{CutCase, READ_BUDGET, StepFixture, run_case};
use crate::driver_exhaustive::{FailureRecord, case_seed, cut_points};
use crate::gc_tally::GcTally;
use crate::oracle::{Failure, Model, apply_step, first_diff, judge_old_or_new, paths_of};
use crate::oracle::{read_state, run_step};
use crate::refusal_check::check_refusal;
use crate::workload::{board_step, edit_doc, panel_json, pick_distinct, push_step};
use crate::{
    Candidate, CandidateConfig, Corpus, CorpusSet, Reproducer, Scoreboard, Step, StoreError,
    WorkloadKind, WorkloadSpec,
};

/// One full-flash run: replayable from these fields alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullFlashParams {
    pub candidate: String,
    pub config: CandidateConfig,
    /// The project copied into slots `f0`, `f1`, … until the store refuses.
    pub corpus: String,
    pub seed: u64,
    /// Steps worked at the edge.
    pub edge_steps: u64,
    /// Cut points sampled per step (fill, edge and recovery steps alike).
    pub cuts_per_step: u64,
    /// Tear models, by name, used in turn; empty = the three guessed ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tears: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FullFlashSummary {
    pub driver: String,
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub corpus: String,
    pub seed: u64,
    /// Copies that fit before the first refusal.
    pub fill_slots: u64,
    /// Steps refused fault-free (fill, edge and recovery).
    pub refusals: u64,
    pub edge_steps: u64,
    pub edge_refused: u64,
    /// Cut cases run (in steps that fit and in refused ones).
    pub cases: u64,
    pub refused_cases: u64,
    pub landed: u64,
    pub torn_erases: u64,
    pub failures: u64,
    pub non_atomic: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first_failure: Option<Failure>,
    /// The first few failures' details (the first is `first_failure`).
    #[serde(default)]
    pub failure_samples: Vec<String>,
    /// Deleting all but one copy, then pushing a new one, worked.
    pub recovered: bool,
    pub gc: GcTally,
    pub error: Option<String>,
}

pub fn full_flash(
    cand: &dyn Candidate,
    p: &FullFlashParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
) -> FullFlashSummary {
    let mut sum = FullFlashSummary {
        driver: "full_flash".into(),
        candidate: cand.name().into(),
        config: Some(p.config.clone()),
        corpus: p.corpus.clone(),
        seed: p.seed,
        ..Default::default()
    };
    match crate::catch_quiet(|| run(cand, p, corpora, &mut sum)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => sum.error = Some(e),
        Err(panic) => fail(&mut sum, Failure::new("panic", panic)),
    }
    if let Some(f) = &sum.first_failure {
        sink.write(
            "failure",
            &FailureRecord {
                driver: "full_flash".into(),
                failure: f.clone(),
                reproducer: Reproducer::FullFlash(p.clone()),
            },
        );
    }
    sink.write("full_flash_summary", &sum);
    sum
}

fn fail(sum: &mut FullFlashSummary, f: Failure) {
    sum.failures += 1;
    *sum.kinds.entry(f.kind.clone()).or_default() += 1;
    if sum.failure_samples.len() < 12 {
        sum.failure_samples
            .push(format!("{}: {}", f.kind, f.detail));
    }
    if sum.first_failure.is_none() {
        sum.first_failure = Some(f);
    }
}

/// Where the run stands: the flash between steps and what it holds.
struct Edge<'a> {
    cand: &'a dyn Candidate,
    p: &'a FullFlashParams,
    tears: Vec<TearModel>,
    flash: NorFlashSim,
    model: Model,
    index: usize,
}

/// How a step went.
enum Outcome {
    Fit,
    Refused,
}

impl Edge<'_> {
    /// Run `step` fault-free on a fresh mount (its cuts first), and adopt
    /// what it leaves. Failures go to `sum`; `Err` stops the run.
    fn step(&mut self, step: Step, sum: &mut FullFlashSummary) -> Result<Outcome, Failure> {
        let cfg = &self.p.config;
        let i = self.index;
        self.index += 1;
        let mut new = self.model.clone();
        apply_step(&mut new, &step);
        let pre = self.flash.clone();
        let mut f = pre.clone();
        f.power_cycle(FaultPlan::none());
        f.set_read_budget(None);
        let mut store = self
            .cand
            .mount(f, cfg)
            .map_err(|(e, _)| Failure::new("unmountable", format!("step {i}: mount: {e}")))?;
        let mount_ops = store.flash_snapshot().ops_since_plan();
        let r = run_step(store.as_mut(), &step);
        let post = sum.gc.unmount(store);
        let step_ops = post.ops_since_plan() - mount_ops;
        let refused = match r {
            Ok(()) => false,
            Err(StoreError::NoSpace) => true,
            Err(e) => {
                return Err(Failure::new(
                    "step_error_without_cut",
                    format!("step {i} {}: {e}", step.label),
                ));
            }
        };
        let fx = StepFixture {
            index: i,
            pre,
            old: self.model.clone(),
            new: new.clone(),
            step: step.clone(),
            next: Step::new("nothing"),
            after_next: new.clone(),
            mount_ops,
            step_ops,
            dry_error: None,
        };
        let points = cut_points(step_ops.saturating_sub(1), Some(self.p.cuts_per_step));
        let cases: Vec<CutCase> = points
            .into_iter()
            .enumerate()
            .map(|(j, k)| CutCase {
                candidate: self.cand.name().into(),
                config: cfg.clone(),
                workload: WorkloadSpec::new(WorkloadKind::Push, "full_flash", self.p.seed),
                step: i,
                cut_after: k,
                tear: self.tears[j % self.tears.len()].name().into(),
                seed: case_seed(self.p.seed, i, k),
                second: None,
            })
            .collect();
        let cand = self.cand;
        // (case, failure, non-atomic, landed, torn erase)
        let outcomes: Vec<(CutCase, Option<Failure>, bool, bool, bool)> = cases
            .into_par_iter()
            .map(|c| {
                if refused {
                    let (f, landed, torn) = refused_case(cand, cfg, &fx, &c);
                    (c, f, false, landed, torn)
                } else {
                    let o = run_case(cand, cfg, &fx, &c);
                    let non_atomic = o.failure.is_none() && !o.atomic;
                    (c, o.failure, non_atomic, o.landed, o.torn_erase)
                }
            })
            .collect();
        for (c, f, non_atomic, landed, torn) in outcomes {
            sum.cases += 1;
            sum.refused_cases += refused as u64;
            sum.landed += landed as u64;
            sum.torn_erases += torn as u64;
            sum.non_atomic += non_atomic as u64;
            if let Some(mut f) = f {
                f.detail = format!(
                    "step {i} {} cut {}/{step_ops} {}: {}",
                    step.label, c.cut_after, c.tear, f.detail
                );
                fail(sum, f);
            }
        }
        if refused {
            sum.refusals += 1;
            let (store, state) =
                check_refusal(cand, cfg, post, &self.model, &new).map_err(|mut f| {
                    f.detail = format!("step {i} {}: {}", step.label, f.detail);
                    f
                })?;
            self.flash = sum.gc.unmount(store);
            self.model = state;
            Ok(Outcome::Refused)
        } else {
            self.flash = post;
            self.model = new;
            Ok(Outcome::Fit)
        }
    }

    /// The whole state against the model after a remount.
    fn check(&mut self, sum: &mut FullFlashSummary) -> Result<(), Failure> {
        let mut f = self.flash.clone();
        f.power_cycle(FaultPlan::none());
        let mut store = self
            .cand
            .mount(f, &self.p.config)
            .map_err(|(e, _)| Failure::new("remount_failed", format!("mount: {e}")))?;
        let paths = self.model.keys().cloned().collect();
        let state = read_state(store.as_mut(), &paths)?;
        sum.gc.add(&store.report());
        if state != self.model {
            return Err(Failure::new(
                "check_wrong_state",
                first_diff(&state, &self.model),
            ));
        }
        Ok(())
    }
}

/// A cut inside a step the store refuses: after it, every path is old or new
/// and a step-atomic store holds exactly the old state.
fn refused_case(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    fx: &StepFixture,
    c: &CutCase,
) -> (Option<Failure>, bool, bool) {
    let r = crate::catch_quiet(|| {
        let mut flash = fx.pre.clone();
        let torn_before = flash.stats().torn_erases;
        flash.set_read_budget(Some(READ_BUDGET));
        flash.power_cycle(FaultPlan {
            cut_after: Some(fx.mount_ops + c.cut_after),
            tear: c.tear_model(),
            seed: c.seed,
        });
        let mut store = cand
            .mount(flash, cfg)
            .map_err(|(e, _)| Failure::new("setup_mount_failed", e.to_string()))?;
        let _ = run_step(store.as_mut(), &fx.step);
        let mut flash = store.into_flash();
        let landed = !flash.is_powered();
        let torn = flash.stats().torn_erases > torn_before;
        flash.power_cycle(FaultPlan::none());
        let mut store = cand
            .mount(flash, cfg)
            .map_err(|(e, _)| Failure::new("unmountable", format!("mount: {e}")))?;
        let state = read_state(store.as_mut(), &paths_of(&fx.old, &fx.new))?;
        judge_old_or_new(&state, &fx.old, &fx.new)?;
        if store.report().step_atomic && state != fx.old {
            return Err(Failure::new(
                "refused_cut_not_old",
                first_diff(&state, &fx.old),
            ));
        }
        Ok((landed, torn))
    });
    match r {
        Ok(Ok((landed, torn))) => (None, landed, torn),
        Ok(Err(f)) => (Some(f), true, false),
        Err(panic) => (Some(Failure::new("panic", panic)), true, false),
    }
}

/// A copy of `corpus` into `slot`, every document prefixed with a line
/// naming the slot: a content-addressed store (T1) would otherwise dedup
/// every copy after the first and never fill (the prefix shifts every chunk
/// boundary, so no chunk repeats either).
fn copy_step(slot: &str, corpus: &Corpus) -> Step {
    let edits: BTreeMap<String, Arc<Vec<u8>>> = corpus
        .docs
        .iter()
        .map(|d| {
            let mut b = format!("// copy {slot}\n").into_bytes();
            b.extend_from_slice(&d.bytes);
            (d.rel.clone(), Arc::new(b))
        })
        .collect();
    let mut s = push_step(&format!("push-{slot}"), slot, corpus, &edits);
    s.ops
        .insert(0, crate::Op::DeletePrefix(format!("/projects/{slot}/")));
    s
}

fn run(
    cand: &dyn Candidate,
    p: &FullFlashParams,
    corpora: &CorpusSet,
    sum: &mut FullFlashSummary,
) -> Result<(), String> {
    let cfg = &p.config;
    let corpus = corpora.get(&p.corpus)?;
    let tears = if p.tears.is_empty() {
        TearModel::ALL.to_vec()
    } else {
        p.tears
            .iter()
            .map(|t| TearModel::from_name(t).ok_or_else(|| format!("unknown tear model {t:?}")))
            .collect::<Result<_, _>>()?
    };
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    let mut e = Edge {
        cand,
        p,
        tears,
        flash,
        model: Model::new(),
        index: 0,
    };
    let mut rng = SimRng::new(p.seed ^ 0xF011_F1A5);
    macro_rules! go {
        ($r:expr) => {
            match $r {
                Ok(v) => v,
                Err(f) => {
                    fail(sum, f);
                    return Ok(());
                }
            }
        };
    }
    go!(e.step(board_step(), sum));
    // Fill.
    let mut slots: Vec<String> = Vec::new();
    loop {
        let slot = format!("f{}", slots.len());
        match go!(e.step(copy_step(&slot, &corpus), sum)) {
            Outcome::Fit => slots.push(slot),
            Outcome::Refused => break,
        }
        if slots.len() > 512 {
            return Err("the store never filled".into());
        }
    }
    sum.fill_slots = slots.len() as u64;
    if slots.is_empty() {
        return Err(format!("{} does not fit once", p.corpus));
    }
    go!(e.check(sum));
    // The edge.
    for _ in 0..p.edge_steps {
        let present: Vec<&String> = slots
            .iter()
            .filter(|s| {
                e.model
                    .keys()
                    .any(|k| k.starts_with(&format!("/projects/{s}/")))
            })
            .collect();
        let slot = present[rng.below(present.len() as u64) as usize].clone();
        let pre = format!("/projects/{slot}/");
        let docs: Vec<String> = e
            .model
            .keys()
            .filter(|k| k.starts_with(&pre))
            .cloned()
            .collect();
        let step = match rng.below(10) {
            0..=3 => {
                let mut s = Step::new("save");
                let k = 1 + rng.below(3) as usize;
                for i in pick_distinct(&mut rng, docs.len(), k) {
                    let d = &docs[i];
                    s.put(d.clone(), Arc::new(edit_doc(d, &e.model[d], &mut rng)));
                }
                s
            }
            4..=5 => {
                let mut s = Step::new("panel");
                s.put(
                    format!("{pre}.lp/panel.json"),
                    Arc::new(panel_json(&mut rng)),
                );
                s
            }
            6..=7 => {
                // A re-push of the slot with a few shaders edited.
                let mut s = Step::new("repush");
                s.delete_prefix(pre.clone());
                for d in &docs {
                    let b = if d.ends_with(".glsl") && rng.chance(1, 4) {
                        Arc::new(edit_doc(d, &e.model[d], &mut rng))
                    } else {
                        e.model[d].clone()
                    };
                    s.put(d.clone(), b);
                }
                s
            }
            8 if present.len() > 1 => {
                let mut s = Step::new("delete");
                s.delete_prefix(pre.clone());
                s
            }
            _ => {
                let slot = format!("f{}", slots.len());
                slots.push(slot.clone());
                copy_step(&slot, &corpus)
            }
        };
        sum.edge_steps += 1;
        if let Outcome::Refused = go!(e.step(step, sum)) {
            sum.edge_refused += 1;
        }
    }
    go!(e.check(sum));
    // Recovery: every copy but the first goes, and a new copy fits.
    let mut s = Step::new("delete-all-but-one");
    for slot in slots.iter().skip(1) {
        s.delete_prefix(format!("/projects/{slot}/"));
    }
    let mut ok = matches!(go!(e.step(s, sum)), Outcome::Fit);
    if ok {
        ok = matches!(go!(e.step(copy_step("r", &corpus), sum)), Outcome::Fit);
    }
    if !ok {
        fail(
            sum,
            Failure::new(
                "no_recovery",
                "deleting all but one copy and pushing one more was refused",
            ),
        );
        return Ok(());
    }
    sum.recovered = true;
    go!(e.check(sum));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::TreeStoreCandidate;

    /// On a small flash T1 fills, refuses, works the edge under cuts and
    /// recovers; nothing fails.
    #[test]
    fn t1_fills_refuses_and_recovers() {
        let p = FullFlashParams {
            candidate: "t1".into(),
            config: CandidateConfig::new(16),
            corpus: "syn:3:900".into(),
            seed: 1,
            edge_steps: 12,
            cuts_per_step: 6,
            tears: vec!["random_bits".into(), "calibrated".into()],
        };
        let s = full_flash(
            &TreeStoreCandidate,
            &p,
            &CorpusSet::new(None),
            &Scoreboard::memory(),
        );
        assert_eq!(s.failures, 0, "{s:?}");
        assert!(
            s.fill_slots >= 2 && s.refusals >= 1 && s.refused_cases > 0,
            "{s:?}"
        );
        assert!(s.recovered && s.cases > 50, "{s:?}");
    }
}
