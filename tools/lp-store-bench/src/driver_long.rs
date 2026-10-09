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
    /// Steps per piece: the walk's draws are reseeded at every multiple, and
    /// a resumable run may pause (and checkpoint) there. 0 = one piece.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub piece_steps: u64,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
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

/// Run the walk to its end (or its first failure); the summary goes to `sink`.
pub fn long_walk(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
) -> LongSummary {
    match long_walk_resumable(cand, p, corpora, sink, None, None) {
        LongEnd::Done(s) => s,
        LongEnd::Paused(..) => unreachable!("no deadline"),
    }
}

/// A walk paused at a piece boundary (`LongParams::piece_steps`): enough,
/// with the flash image beside it, to go on (`long --checkpoint-dir`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LongCheckpoint {
    pub params: LongParams,
    pub next_step: u64,
    pub summary: LongSummary,
    pub model: BTreeMap<String, Vec<u8>>,
    /// Erases per sector before the checkpoint (a restored flash counts its
    /// own from zero).
    pub erases_per_sector: Vec<u32>,
}

/// How a resumable walk stopped.
pub enum LongEnd {
    Done(LongSummary),
    /// The deadline passed at a piece boundary: the checkpoint and the
    /// flash image (sector by sector; weak bits are frozen at the values
    /// they read — they only ever sit in sectors the store re-erases).
    Paused(Box<LongCheckpoint>, Vec<u8>),
}

/// [`long_walk`], from a checkpoint if given, pausing at the first piece
/// boundary past `deadline`. The walk's draws are reseeded at every piece
/// boundary, so a walk run in pieces is the walk run in one go.
pub fn long_walk_resumable(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
    resume: Option<(LongCheckpoint, Vec<u8>)>,
    deadline: Option<std::time::Instant>,
) -> LongEnd {
    let (mut sum, start) = match resume {
        Some((ck, image)) => {
            let start = Start {
                flash: restore(&p.config, &image),
                model: ck
                    .model
                    .into_iter()
                    .map(|(k, v)| (k, std::sync::Arc::new(v)))
                    .collect(),
                next_step: ck.next_step,
                base_erases: ck.erases_per_sector,
            };
            (ck.summary, Some(start))
        }
        None => (
            LongSummary {
                driver: "long".into(),
                candidate: cand.name().into(),
                config: Some(p.config.clone()),
                seed: p.seed,
                ..Default::default()
            },
            None,
        ),
    };
    let base = start.as_ref().map(|s| s.base_erases.clone());
    match crate::catch_quiet(|| walk(cand, p, corpora, &mut sum, start, None, deadline)) {
        Ok(Ok(WalkEnd::Paused(flash, model, next_step))) => {
            let mut erases = flash.stats().erases_per_sector.clone();
            if let Some(b) = &base {
                for (e, b) in erases.iter_mut().zip(b) {
                    *e += b;
                }
            }
            let ck = LongCheckpoint {
                params: p.clone(),
                next_step,
                summary: sum,
                model: model.into_iter().map(|(k, v)| (k, (*v).clone())).collect(),
                erases_per_sector: erases,
            };
            return LongEnd::Paused(Box::new(ck), image_of(&flash));
        }
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
    LongEnd::Done(sum)
}

/// A walk's starting point other than a fresh format.
struct Start {
    flash: NorFlashSim,
    model: Model,
    next_step: u64,
    base_erases: Vec<u32>,
}

/// How [`walk`] stopped.
enum WalkEnd {
    Done,
    /// At `stop_before`: the flash, the state and the step.
    Prefix(NorFlashSim, Model, crate::Step),
    /// At a piece boundary past the deadline: the flash, the state, the
    /// next step's index.
    Paused(NorFlashSim, Model, u64),
}

/// The flash's cells, sector after sector.
fn image_of(f: &NorFlashSim) -> Vec<u8> {
    let g = f.geometry();
    let mut out = vec![0u8; (g.sector_count * g.sector_size) as usize];
    f.peek(0, &mut out);
    out
}

/// A flash holding `image` (counters from zero).
fn restore(cfg: &CandidateConfig, image: &[u8]) -> NorFlashSim {
    let mut f = NorFlashSim::new(cfg.geometry());
    let ss = cfg.geometry().sector_size as usize;
    for (s, cells) in image.chunks(ss).enumerate() {
        if cells.iter().any(|&b| b != 0xFF) {
            f.program((s * ss) as u32, cells)
                .expect("program a restored image");
        }
    }
    f.reset_stats();
    f.set_panic_on_violation(true);
    f
}

/// The walk's draws for step `i`'s piece (piece 0's are the walk's seed).
fn piece_rng(p: &LongParams, i: u64) -> SimRng {
    let piece = if p.piece_steps == 0 {
        0
    } else {
        i / p.piece_steps
    };
    SimRng::new(p.seed ^ 0x10_C6_5EED ^ piece.wrapping_mul(0x9E37_79B9_7F4A_7C15))
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
    let out = walk(cand, p, corpora, &mut sum, None, Some(before), None)?;
    match (out, sum.first_failure) {
        (WalkEnd::Prefix(f, m, s), _) => Ok((f, m, s)),
        (_, Some(f)) => Err(format!("failed first: {}: {}", f.kind, f.detail)),
        _ => Err(format!("the walk has no step {before}")),
    }
}

/// Run the walk (from `start`, or a fresh format); with `stop_before`, stop
/// (unmounted) at that step; with `deadline`, pause at the first piece
/// boundary past it.
fn walk(
    cand: &dyn Candidate,
    p: &LongParams,
    corpora: &CorpusSet,
    sum: &mut LongSummary,
    start: Option<Start>,
    stop_before: Option<u64>,
    deadline: Option<std::time::Instant>,
) -> Result<WalkEnd, String> {
    let cfg = &p.config;
    let tears = p.tear_models()?;
    let (flash, mut model, first, base) = match start {
        Some(s) => (s.flash, s.model, s.next_step, Some(s.base_erases)),
        None => {
            let mut flash = NorFlashSim::new(cfg.geometry());
            flash.set_panic_on_violation(p.wear.is_empty());
            for w in &p.wear {
                flash.add_wear_out(w.wear_out());
            }
            cand.format(&mut flash, cfg)
                .map_err(|e| format!("format: {e}"))?;
            (flash, Model::new(), 0, None)
        }
    };
    let mut rng = piece_rng(p, first);
    let mut store = match mount(cand, cfg, flash, "unmountable") {
        Ok(s) => s,
        Err(f) => {
            fail(sum, f);
            return Ok(WalkEnd::Done);
        }
    };
    for i in first..p.steps {
        if p.piece_steps > 0 && i % p.piece_steps == 0 && i > first {
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                return Ok(WalkEnd::Paused(sum.gc.unmount(store), model, i));
            }
            rng = piece_rng(p, i);
        }
        let step = if i == 0 {
            board_step()
        } else if p.edit_mix {
            edit_step(&mut rng, &model, corpora, &p.corpora, i)?
        } else {
            next_step(&mut rng, &model, corpora, &p.corpora)?
        };
        if stop_before == Some(i) {
            return Ok(WalkEnd::Prefix(sum.gc.unmount(store), model, step));
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
                return Ok(WalkEnd::Done);
            }
        };
        if p.check_every > 0 && (i + 1) % p.check_every == 0 {
            store = match check(cand, cfg, store, &model, sum) {
                Ok(s) => s,
                Err(f) => {
                    fail(sum, f);
                    return Ok(WalkEnd::Done);
                }
            };
        }
    }
    match check(cand, cfg, store, &model, sum) {
        Ok(store) => {
            let flash = sum.gc.unmount(store);
            if let Some(b) = base {
                let e: Vec<u32> = flash
                    .stats()
                    .erases_per_sector
                    .iter()
                    .zip(&b)
                    .map(|(a, b)| a + b)
                    .collect();
                sum.gc.erases_total = e.iter().map(|&x| u64::from(x)).sum();
                sum.gc.erases_max = e.iter().copied().max().unwrap_or(0);
            }
        }
        Err(f) => fail(sum, f),
    }
    Ok(WalkEnd::Done)
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
            piece_steps: 0,
        }
    }

    /// A walk paused at every piece boundary and resumed from its
    /// checkpoint (through JSON) is the walk run in one go.
    #[test]
    fn a_walk_in_pieces_is_the_walk_in_one_go() {
        let set = CorpusSet::new(None);
        let sink = Scoreboard::memory();
        let p = LongParams {
            piece_steps: 100,
            ..params(12)
        };
        let whole = long_walk(&TreeStoreCandidate, &p, &set, &sink);
        let mut resume = None;
        let mut pauses = 0;
        let pieces = loop {
            let past = Some(std::time::Instant::now());
            match long_walk_resumable(&TreeStoreCandidate, &p, &set, &sink, resume, past) {
                LongEnd::Done(s) => break s,
                LongEnd::Paused(ck, image) => {
                    pauses += 1;
                    let ck: LongCheckpoint =
                        serde_json::from_str(&serde_json::to_string(&*ck).unwrap()).unwrap();
                    resume = Some((ck, image));
                }
            }
        };
        assert_eq!(pauses, 3);
        let key = |s: &LongSummary| {
            (
                s.steps_run,
                s.cuts,
                s.landed,
                s.checks,
                s.failures,
                s.steps_no_space,
                s.gc.gc_runs,
                s.gc.erases_total,
                s.logical_bytes,
            )
        };
        assert_eq!(key(&pieces), key(&whole), "{pieces:?}\n{whole:?}");
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
