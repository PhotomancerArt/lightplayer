//! Mount fuzzing (M3 P3): mount images no workload leaves behind, and hold
//! the store to what it may do with them.
//!
//! Kinds of image, in turn:
//! - `garbage`: every byte random;
//! - `mutated`: a store at a random point of a random history (seeded walks
//!   on a mounted store), then one to three of: random bit flips, a run of
//!   bytes set to `0x00` or `0xFF`, two sectors swapped, a sector copied over
//!   another, a written sector's body (past its first 24 bytes) randomized;
//! - `cut`: a store cut at a random operation of a random step of a random
//!   history, under a random tear model;
//! - `newer_version` (T1 only): a store whose one trusted sector header
//!   claims a newer format version (CRC resealed) — a rolled-back core
//!   reading a newer core's store;
//! - `stale_tail` (T1 only, **non-physical**): a committed store whose two
//!   newest sectors carry a few non-`0xFF` bytes in the erased tail past
//!   their last record — what a bad erase or a foreign writer would leave;
//!   no tear model produces it. The store must mount it at that committed
//!   state, take the history's next step, and never program a byte the
//!   tail held (a program asking a cleared bit to become 1:
//!   `program_over_unerased`) — the guarantee the head-resume tail check
//!   keeps.
//!
//! Pass: mount never panics and never loops (the read watchdog); when it
//! mounts, the state it reads is one the history committed (a `cut` image:
//! the interrupted step's old or new state, per path for a store that is
//! not step-atomic), with no read error; a `newer_version` image is refused
//! as `Unsupported` (never mounted, never "no store"); and a format
//! afterwards gives an empty store that takes a write. A case is replayable
//! from the params and its index (`Reproducer::Fuzz`).

use std::collections::BTreeMap;
use std::sync::Arc;

use lp_nor_sim::{FaultPlan, NorFlashSim, SimRng, TearModel};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cut_case::READ_BUDGET;
use crate::driver_exhaustive::FailureRecord;
use crate::driver_random::next_step;
use crate::oracle::{
    Failure, Model, apply_step, first_diff, judge_old_or_new, read_state, run_step,
};
use crate::workload::board_step;
use crate::{
    Candidate, CandidateConfig, CandidateStore, CorpusSet, Reproducer, Scoreboard, Step, StoreError,
};

/// One fuzz run: every case is a function of these and its index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzParams {
    pub candidate: String,
    pub config: CandidateConfig,
    pub corpora: Vec<String>,
    pub seed: u64,
    pub cases: u64,
    /// Histories built (each case picks one).
    pub histories: u64,
    pub history_steps: u64,
    /// Tear models a `cut` image draws from; empty = the three guessed ones
    /// and `calibrated`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tears: Vec<String>,
    /// Replay: run only this case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_case: Option<u64>,
}

/// Per kind of image.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FuzzKindCount {
    pub cases: u64,
    pub mounted: u64,
    pub refused: u64,
    pub failures: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FuzzSummary {
    pub driver: String,
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub seed: u64,
    pub cases: u64,
    pub by_kind: BTreeMap<String, FuzzKindCount>,
    /// Mutations applied, by name (a `mutated` image takes one to three).
    pub mutations: BTreeMap<String, u64>,
    pub failures: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first_failure: Option<Failure>,
    pub first_failing_case: Option<u64>,
    pub error: Option<String>,
}

pub fn fuzz(
    cand: &dyn Candidate,
    p: &FuzzParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
) -> FuzzSummary {
    let mut sum = FuzzSummary {
        driver: "fuzz".into(),
        candidate: cand.name().into(),
        config: Some(p.config.clone()),
        seed: p.seed,
        ..Default::default()
    };
    if let Err(e) = run(cand, p, corpora, sink, &mut sum) {
        sum.error = Some(e);
    }
    if p.only_case.is_none() {
        sink.write("fuzz_summary", &sum);
    }
    sum
}

/// A store's committed history: the flash before each step, the state
/// committed before it, and the step.
struct History {
    /// `states[j]` = what the store held before step `j`; the last entry is
    /// the state after the last step.
    states: Vec<Model>,
    pres: Vec<NorFlashSim>,
    steps: Vec<Step>,
    last: NorFlashSim,
}

/// What one case did.
struct CaseResult {
    kind: &'static str,
    mutations: Vec<&'static str>,
    mounted: bool,
    failure: Option<Failure>,
}

fn run(
    cand: &dyn Candidate,
    p: &FuzzParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
    sum: &mut FuzzSummary,
) -> Result<(), String> {
    let tears: Vec<TearModel> = if p.tears.is_empty() {
        let mut t = TearModel::ALL.to_vec();
        t.push(TearModel::Calibrated);
        t
    } else {
        p.tears
            .iter()
            .map(|t| TearModel::from_name(t).ok_or_else(|| format!("unknown tear model {t:?}")))
            .collect::<Result<_, _>>()?
    };
    let histories: Vec<History> = (0..p.histories.max(1))
        .into_par_iter()
        .map(|h| history(cand, p, corpora, h))
        .collect::<Result<_, _>>()?;
    let t1 = cand.name() == "t1";
    let cases: Vec<u64> = match p.only_case {
        Some(c) => vec![c],
        None => (0..p.cases).collect(),
    };
    let results: Vec<(u64, CaseResult)> = cases
        .into_par_iter()
        .map(|c| (c, one_case(cand, p, &histories, &tears, t1, c)))
        .collect();
    for (c, r) in results {
        sum.cases += 1;
        let k = sum.by_kind.entry(r.kind.into()).or_default();
        k.cases += 1;
        k.mounted += r.mounted as u64;
        k.refused += !r.mounted as u64;
        for m in r.mutations {
            *sum.mutations.entry(m.into()).or_default() += 1;
        }
        if let Some(f) = r.failure {
            k.failures += 1;
            sum.failures += 1;
            *sum.kinds.entry(f.kind.clone()).or_default() += 1;
            if sum.first_failure.is_none() {
                sum.first_failure = Some(f.clone());
                sum.first_failing_case = Some(c);
            }
            if p.only_case.is_none() && sum.failures <= 10 {
                sink.write(
                    "failure",
                    &FailureRecord {
                        driver: "fuzz".into(),
                        failure: f,
                        reproducer: Reproducer::Fuzz(FuzzParams {
                            only_case: Some(c),
                            ..p.clone()
                        }),
                    },
                );
            }
        }
    }
    Ok(())
}

/// History `h`: a seeded walk on one mounted store (refusals checked and
/// adopted as the random driver does).
fn history(
    cand: &dyn Candidate,
    p: &FuzzParams,
    corpora: &CorpusSet,
    h: u64,
) -> Result<History, String> {
    let cfg = &p.config;
    let mut rng = SimRng::new(p.seed ^ (h + 1).wrapping_mul(0xF022_A11E));
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    let mut model = Model::new();
    let mut out = History {
        states: Vec::new(),
        pres: Vec::new(),
        steps: Vec::new(),
        last: flash.clone(),
    };
    for i in 0..p.history_steps {
        let step = if i == 0 {
            board_step()
        } else {
            next_step(&mut rng, &model, corpora, &p.corpora)?
        };
        let mut store = cand
            .mount(flash.clone(), cfg)
            .map_err(|(e, _)| format!("history {h} step {i}: mount: {e}"))?;
        match run_step(store.as_mut(), &step) {
            Ok(()) => {
                out.states.push(model.clone());
                out.pres.push(flash);
                apply_step(&mut model, &step);
                out.steps.push(step);
                flash = store.into_flash();
            }
            // Refused: the flash before the step goes on (it is what a
            // remount after the refusal holds, `check_refusal` proves).
            Err(StoreError::NoSpace) => drop(store),
            Err(e) => return Err(format!("history {h} step {i}: {e}")),
        }
    }
    out.states.push(model);
    out.last = flash;
    Ok(out)
}

fn one_case(
    cand: &dyn Candidate,
    p: &FuzzParams,
    histories: &[History],
    tears: &[TearModel],
    t1: bool,
    c: u64,
) -> CaseResult {
    let kinds: &[&'static str] = if t1 {
        &["garbage", "mutated", "cut", "newer_version", "stale_tail"]
    } else {
        &["garbage", "mutated", "cut"]
    };
    let kind = kinds[(c % kinds.len() as u64) as usize];
    let mut res = CaseResult {
        kind,
        mutations: Vec::new(),
        mounted: false,
        failure: None,
    };
    let r = crate::catch_quiet(|| case_inner(cand, p, histories, tears, kind, c, &mut res));
    match r {
        Ok(Ok(())) => {}
        Ok(Err(f)) => res.failure = Some(f),
        Err(panic) => res.failure = Some(Failure::new("panic", panic)),
    }
    if let Some(f) = &mut res.failure {
        f.detail = format!("case {c} {kind} {:?}: {}", res.mutations, f.detail);
    }
    res
}

fn case_inner(
    cand: &dyn Candidate,
    p: &FuzzParams,
    histories: &[History],
    tears: &[TearModel],
    kind: &'static str,
    c: u64,
    res: &mut CaseResult,
) -> Result<(), Failure> {
    let cfg = &p.config;
    let mut rng = SimRng::new(p.seed ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xF0_22);
    let hist = &histories[rng.below(histories.len() as u64) as usize];
    if kind == "stale_tail" {
        return stale_tail_case(cand, cfg, hist, &mut rng, res);
    }
    // The image, and the states a mount of it may show.
    let (mut image, allowed): (NorFlashSim, Allowed) = match kind {
        "garbage" => (
            NorFlashSim::garbage(cfg.geometry(), rng.next_u64()),
            Allowed::Any,
        ),
        "mutated" | "newer_version" => {
            let j = rng.below(hist.states.len() as u64) as usize;
            let mut f = if j < hist.pres.len() {
                hist.pres[j].clone()
            } else {
                hist.last.clone()
            };
            let allowed = Allowed::OneOf(hist.states[..=j].to_vec());
            if kind == "newer_version" {
                if !newer_version(&mut f, &mut rng) {
                    return Ok(());
                }
                (f, Allowed::Refused)
            } else {
                for _ in 0..1 + rng.below(3) {
                    res.mutations.push(mutate(&mut f, &mut rng));
                }
                (f, allowed)
            }
        }
        _ => {
            if hist.steps.is_empty() {
                return Ok(());
            }
            let j = rng.below(hist.steps.len() as u64) as usize;
            let (old, step) = (&hist.states[j], &hist.steps[j]);
            let mut new = old.clone();
            apply_step(&mut new, step);
            let tear = tears[rng.below(tears.len() as u64) as usize];
            (
                cut_image(cand, cfg, &hist.pres[j], step, tear, &mut rng)?,
                Allowed::OldOrNew(old.clone(), new),
            )
        }
    };
    image.set_panic_on_violation(false);
    image.power_cycle(FaultPlan::none());
    image.set_read_budget(Some(READ_BUDGET));
    let flash = match cand.mount(image, cfg) {
        Err((e, f)) => {
            let text = e.to_string();
            if text.contains("Watchdog") {
                return Err(Failure::new("loop", text));
            }
            if matches!(allowed, Allowed::Refused) && !text.contains("Unsupported") {
                return Err(Failure::new("newer_version_not_refused", text));
            }
            f
        }
        Ok(mut store) => {
            res.mounted = true;
            if matches!(allowed, Allowed::Refused) {
                return Err(Failure::new("newer_version_mounted", "mounted"));
            }
            judge(store.as_mut(), &allowed)?;
            store.into_flash()
        }
    };
    format_after(cand, cfg, flash)
}

/// A `stale_tail` case: committed state `j` with stale bytes in its newest
/// sectors' tails; it mounts at that state, takes step `j` (or a probe at
/// the history's end) without programming over them, and then a format
/// works.
fn stale_tail_case(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    hist: &History,
    rng: &mut SimRng,
    res: &mut CaseResult,
) -> Result<(), Failure> {
    let j = rng.below(hist.states.len() as u64) as usize;
    let mut image = if j < hist.pres.len() {
        hist.pres[j].clone()
    } else {
        hist.last.clone()
    };
    if !stale_tail(&mut image, rng) {
        return Ok(());
    }
    image.set_panic_on_violation(false);
    image.power_cycle(FaultPlan::none());
    image.set_read_budget(Some(READ_BUDGET));
    let mut store = match cand.mount(image, cfg) {
        Ok(s) => s,
        Err((e, _)) => {
            let text = e.to_string();
            let kind = if text.contains("Watchdog") {
                "loop"
            } else {
                "stale_tail_unmountable"
            };
            return Err(Failure::new(kind, text));
        }
    };
    res.mounted = true;
    let old = &hist.states[j];
    judge(store.as_mut(), &Allowed::OneOf(vec![old.clone()]))?;
    let before = store.flash_snapshot().stats().violations_0_to_1;
    let step = match hist.steps.get(j) {
        Some(s) => s.clone(),
        None => {
            let mut s = Step::new("probe");
            s.put("/stale-probe.json", Arc::new(b"{\"stale\": 1}".to_vec()));
            s
        }
    };
    let mut new = old.clone();
    apply_step(&mut new, &step);
    let expect = match run_step(store.as_mut(), &step) {
        Ok(()) => new,
        Err(StoreError::NoSpace) => old.clone(),
        Err(e) => return Err(Failure::new("step_error_without_cut", e.to_string())),
    };
    let after = store.flash_snapshot().stats().violations_0_to_1;
    if after > before {
        return Err(Failure::new(
            "program_over_unerased",
            format!(
                "{} byte(s) programmed over a tail that did not read 0xFF",
                after - before
            ),
        ));
    }
    let paths = old.keys().chain(expect.keys()).cloned().collect();
    let state = read_state(store.as_mut(), &paths)?;
    if state != expect {
        return Err(Failure::new(
            "stale_tail_wrong_state",
            first_diff(&state, &expect),
        ));
    }
    format_after(cand, cfg, store.into_flash())
}

/// What a mounted image may hold.
enum Allowed {
    /// No history (garbage): any state that reads back without an error.
    Any,
    /// One of these committed states.
    OneOf(Vec<Model>),
    /// The interrupted step's old or new state (per path if the store is not
    /// step-atomic).
    OldOrNew(Model, Model),
    /// Must not mount at all (`newer_version`).
    Refused,
}

fn judge(store: &mut dyn CandidateStore, allowed: &Allowed) -> Result<(), Failure> {
    let paths = match allowed {
        Allowed::OneOf(states) => states.iter().flat_map(|s| s.keys().cloned()).collect(),
        Allowed::OldOrNew(o, n) => o.keys().chain(n.keys()).cloned().collect(),
        _ => Default::default(),
    };
    let state = read_state(store, &paths).map_err(|mut f| {
        f.kind = "read_error_after_mount".into();
        f
    })?;
    match allowed {
        Allowed::OneOf(states) => {
            if !states.contains(&state) {
                return Err(Failure::new(
                    "not_a_committed_state",
                    format!("{} paths, none of {} states", state.len(), states.len()),
                ));
            }
        }
        Allowed::OldOrNew(o, n) => {
            let atomic = judge_old_or_new(&state, o, n)?;
            if store.report().step_atomic && !atomic {
                return Err(Failure::new("cut_not_old_or_new", "a step-atomic store"));
            }
        }
        _ => {}
    }
    Ok(())
}

/// After any image: format, mount, empty, takes a write.
fn format_after(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    mut flash: NorFlashSim,
) -> Result<(), Failure> {
    flash.power_cycle(FaultPlan::none());
    flash.set_read_budget(None);
    cand.format(&mut flash, cfg)
        .map_err(|e| Failure::new("format_after_failed", e.to_string()))?;
    let mut store = cand
        .mount(flash, cfg)
        .map_err(|(e, _)| Failure::new("format_after_failed", format!("mount: {e}")))?;
    let listed = store
        .list("/")
        .map_err(|e| Failure::new("format_after_failed", format!("list: {e}")))?;
    if !listed.is_empty() {
        return Err(Failure::new(
            "format_after_failed",
            format!("{} files after a format", listed.len()),
        ));
    }
    let mut s = Step::new("probe");
    s.put("/probe.json", Arc::new(b"{\"probe\": 1}".to_vec()));
    run_step(store.as_mut(), &s).map_err(|e| Failure::new("format_after_failed", e.to_string()))?;
    match store.get("/probe.json") {
        Ok(Some(b)) if b == b"{\"probe\": 1}" => Ok(()),
        other => Err(Failure::new(
            "format_after_failed",
            format!("probe read {other:?}"),
        )),
    }
}

/// The flash after `step` on a mount of `pre`, cut at a random op of it.
fn cut_image(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    pre: &NorFlashSim,
    step: &Step,
    tear: TearModel,
    rng: &mut SimRng,
) -> Result<NorFlashSim, Failure> {
    let mut f = pre.clone();
    f.power_cycle(FaultPlan::none());
    let mut store = cand
        .mount(f, cfg)
        .map_err(|(e, _)| Failure::new("unmountable", format!("pre mount: {e}")))?;
    let mount_ops = store.flash_snapshot().ops_since_plan();
    let _ = run_step(store.as_mut(), step);
    let total = store.into_flash().ops_since_plan() - mount_ops;
    let mut f = pre.clone();
    f.power_cycle(FaultPlan::cut(
        mount_ops + rng.below(total + 1),
        tear,
        rng.next_u64(),
    ));
    let mut store = cand
        .mount(f, cfg)
        .map_err(|(e, _)| Failure::new("unmountable", format!("pre mount: {e}")))?;
    let _ = run_step(store.as_mut(), step);
    Ok(store.into_flash())
}

/// One random mutation of `f` (cold: no op counted against a plan).
fn mutate(f: &mut NorFlashSim, rng: &mut SimRng) -> &'static str {
    let g = f.geometry();
    let ss = g.sector_size as usize;
    let written: Vec<u32> = (0..g.sector_count)
        .filter(|&s| !f.sector_is_blank(s))
        .collect();
    let pick = |rng: &mut SimRng, among: &[u32]| -> u32 {
        if among.is_empty() {
            rng.below(g.sector_count as u64) as u32
        } else {
            among[rng.below(among.len() as u64) as usize]
        }
    };
    match rng.below(6) {
        0 => {
            let s = pick(rng, &written);
            let n = 1 + rng.below(8);
            rewrite(f, s, |b| {
                for _ in 0..n {
                    let i = rng.below(ss as u64) as usize;
                    b[i] ^= 1 << rng.below(8);
                }
            });
            "bit_flips"
        }
        1 | 2 => {
            let s = pick(rng, &written);
            let at = rng.below(ss as u64) as usize;
            let len = (1 + rng.below(256) as usize).min(ss - at);
            let v = if rng.chance(1, 2) { 0x00 } else { 0xFF };
            rewrite(f, s, |b| b[at..at + len].fill(v));
            if v == 0 { "zero_run" } else { "ff_run" }
        }
        3 => {
            let a = pick(rng, &written);
            let b = rng.below(g.sector_count as u64) as u32;
            let (da, db) = (sector_bytes(f, a), sector_bytes(f, b));
            rewrite(f, a, |x| x.copy_from_slice(&db));
            rewrite(f, b, |x| x.copy_from_slice(&da));
            "swap_sectors"
        }
        4 => {
            let a = pick(rng, &written);
            let b = rng.below(g.sector_count as u64) as u32;
            let da = sector_bytes(f, a);
            rewrite(f, b, |x| x.copy_from_slice(&da));
            "duplicate_sector"
        }
        _ => {
            let s = pick(rng, &written);
            rewrite(f, s, |b| {
                for x in &mut b[24..] {
                    *x = rng.next_u8();
                }
            });
            "scramble_body"
        }
    }
}

/// T1: raise one trusted sector header's version past this code's, CRC
/// resealed. `false` when no sector carries a header.
fn newer_version(f: &mut NorFlashSim, rng: &mut SimRng) -> bool {
    let g = f.geometry();
    let magic = 0x3153_544Cu32.to_le_bytes();
    let trusted: Vec<u32> = (0..g.sector_count)
        .filter(|&s| {
            let mut h = [0u8; 24];
            f.peek(s * g.sector_size, &mut h);
            h[..4] == magic
                && u16::from_le_bytes([h[4], h[5]]) == lp_tree_store::FORMAT_VERSION
                && lp_crc32::crc32(&h[..20]).to_le_bytes() == h[20..24]
        })
        .collect();
    if trusted.is_empty() {
        return false;
    }
    let s = trusted[rng.below(trusted.len() as u64) as usize];
    let v = lp_tree_store::FORMAT_VERSION + 1 + rng.below(4) as u16;
    rewrite(f, s, |b| {
        b[4..6].copy_from_slice(&v.to_le_bytes());
        let crc = lp_crc32::crc32(&b[..20]);
        b[20..24].copy_from_slice(&crc.to_le_bytes());
    });
    true
}

/// T1, non-physical: in the two newest trusted sectors whose tail past the
/// last record reads erased, set a short run of bytes (each with a cleared
/// bit) at a random offset in the first 64 bytes of that tail. `false` when
/// no sector has such a tail.
fn stale_tail(f: &mut NorFlashSim, rng: &mut SimRng) -> bool {
    let ss = f.geometry().sector_size;
    let mut image = vec![0u8; (f.geometry().sector_count * ss) as usize];
    f.peek(0, &mut image);
    let Ok(img) = lp_tree_store::StoreImage::open(&image, Some(ss)) else {
        return false;
    };
    let mut open: Vec<(u32, u32, u32)> = img
        .report()
        .sectors
        .iter()
        .filter(|s| s.tail_erased && s.records_end < ss)
        .filter_map(|s| s.header.as_ref().map(|h| (h.seq, s.index, s.records_end)))
        .collect();
    open.sort_unstable_by(|a, b| b.cmp(a));
    open.truncate(2);
    for &(_, s, end) in &open {
        let at = (end + rng.below(u64::from((ss - end).min(64))) as u32) as usize;
        let len = (1 + rng.below(32) as usize).min(ss as usize - at);
        let bytes: Vec<u8> = (0..len)
            .map(|_| rng.next_u8() & !(1 << rng.below(8)))
            .collect();
        rewrite(f, s, |b| b[at..at + len].copy_from_slice(&bytes));
    }
    !open.is_empty()
}

fn sector_bytes(f: &NorFlashSim, s: u32) -> Vec<u8> {
    let ss = f.geometry().sector_size;
    let mut b = vec![0u8; ss as usize];
    f.peek(s * ss, &mut b);
    b
}

/// Replace sector `s`'s bytes with `edit` of them (erase, then program).
fn rewrite(f: &mut NorFlashSim, s: u32, edit: impl FnOnce(&mut [u8])) {
    let ss = f.geometry().sector_size;
    let mut b = sector_bytes(f, s);
    edit(&mut b);
    let plan = f.plan();
    f.power_cycle(FaultPlan::none());
    f.erase_sector(s).expect("erase");
    f.program(s * ss, &b).expect("program");
    f.power_cycle(plan);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout, TreeStoreCandidate};

    fn params(cand: &str) -> FuzzParams {
        FuzzParams {
            candidate: cand.into(),
            config: CandidateConfig::new(16),
            corpora: vec!["syn:2:300".into(), "syn:3:500".into()],
            seed: 7,
            cases: 160,
            histories: 3,
            history_steps: 10,
            tears: vec![],
            only_case: None,
        }
    }

    /// The quick seeded subset: T1 survives every kind of image; every kind
    /// ran and some mounted.
    #[test]
    fn t1_survives_the_quick_fuzz() {
        let s = fuzz(
            &TreeStoreCandidate,
            &params("t1"),
            &CorpusSet::new(None),
            &Scoreboard::memory(),
        );
        assert_eq!(s.failures, 0, "{s:#?}");
        assert_eq!(s.by_kind.len(), 5, "{s:?}");
        assert!(s.by_kind.values().all(|k| k.cases >= 30), "{s:?}");
        assert!(
            s.by_kind["mutated"].mounted > 0 && s.by_kind["cut"].mounted > 0,
            "{s:?}"
        );
        assert_eq!(s.by_kind["newer_version"].mounted, 0, "{s:?}");
        assert!(s.mutations.len() == 6, "{s:?}");
    }

    /// The reference passes too; a failing case replays alone.
    #[test]
    fn the_reference_passes_and_a_case_replays_alone() {
        let set = CorpusSet::new(None);
        let s = fuzz(
            &MemCandidate::new(MemLayout::PingPong),
            &FuzzParams {
                cases: 60,
                ..params("mem")
            },
            &set,
            &Scoreboard::memory(),
        );
        assert_eq!(s.failures, 0, "{s:#?}");
        let one = fuzz(
            &TreeStoreCandidate,
            &FuzzParams {
                only_case: Some(5),
                ..params("t1")
            },
            &set,
            &Scoreboard::memory(),
        );
        assert_eq!(one.cases, 1);
    }
}
