//! One power-cut case, end to end: fork the pre-step flash, cut, remount,
//! judge, prove the store still writes. Also the per-step fixtures the sweeps
//! fork from.

use lp_nor_sim::{FaultPlan, NorFlashSim, TearModel};
use serde::{Deserialize, Serialize};

use crate::oracle::{
    Failure, Model, apply_step, first_diff, judge_old_or_new, paths_of, read_state, run_step,
};
use crate::{
    Candidate, CandidateConfig, CandidateStore, Step, StoreError, Workload, WorkloadSpec,
    catch_quiet,
};

/// A store that reads more than this many times between power cycles is
/// treated as looping (the read watchdog fires).
pub const READ_BUDGET: u64 = 20_000_000;

/// A second cut, for double-cut cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecondCut {
    /// During the mount (and any repair it does) after the first cut, at its
    /// `at`-th operation.
    Mount { at: u64 },
    /// During the re-run of the interrupted step, at its `at`-th operation.
    Rerun { at: u64 },
}

/// Everything that names one case: enough to replay it exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutCase {
    pub candidate: String,
    pub config: CandidateConfig,
    pub workload: WorkloadSpec,
    pub step: usize,
    /// Operations of the step (counted after its mount) that complete first.
    pub cut_after: u64,
    pub tear: String,
    pub seed: u64,
    #[serde(default)]
    pub second: Option<SecondCut>,
}

impl CutCase {
    pub fn tear_model(&self) -> TearModel {
        TearModel::from_name(&self.tear).unwrap_or(TearModel::Clean)
    }
}

/// How a case ended.
#[derive(Clone, Debug)]
pub struct CaseOutcome {
    pub failure: Option<Failure>,
    /// The state after the cut was exactly old or exactly new.
    pub atomic: bool,
    /// The cut fired (k < the step's op count).
    pub landed: bool,
    /// The op the cut tore was a sector erase (not a program page).
    pub torn_erase: bool,
    /// The flash after recovery, re-run, next step and remount (for the random
    /// driver to continue from).
    pub final_flash: Option<NorFlashSim>,
}

/// A step's pre-state, its expected states, and its op counts.
#[derive(Clone, Debug)]
pub struct StepFixture {
    pub index: usize,
    pub pre: NorFlashSim,
    pub old: Model,
    pub new: Model,
    pub step: Step,
    /// The step run after recovery, to prove the store still writes.
    pub next: Step,
    pub after_next: Model,
    pub mount_ops: u64,
    pub step_ops: u64,
    /// The step failed fault-free (usually `NoSpace`): not sweepable.
    pub dry_error: Option<StoreError>,
}

/// A tiny step that proves the store still takes writes.
pub fn probe_step(seed: u64) -> Step {
    let mut s = Step::new("probe");
    s.put(
        "/probe.json",
        std::sync::Arc::new(format!("{{\n  \"probe\": {seed}\n}}\n").into_bytes()),
    );
    s
}

/// Format a store and run `workload` fault-free, remounting before every step
/// (each step's pre-state is a cold flash), keeping a fixture per step.
pub fn prepare_fixtures(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    workload: &Workload,
) -> Result<Vec<StepFixture>, String> {
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    let mut model = Model::new();
    let mut fixtures = Vec::new();
    for (i, step) in workload.steps.iter().enumerate() {
        let pre = flash.clone();
        let (post, mount_ops, step_ops, err) = dry_run(cand, cfg, &pre, step)?;
        let old = model.clone();
        let mut new = model.clone();
        apply_step(&mut new, step);
        let next = workload
            .steps
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| probe_step(i as u64));
        let mut after_next = new.clone();
        apply_step(&mut after_next, &next);
        let stop = err.is_some();
        fixtures.push(StepFixture {
            index: i,
            pre,
            old,
            new: new.clone(),
            step: step.clone(),
            next,
            after_next,
            mount_ops,
            step_ops,
            dry_error: err,
        });
        if stop {
            break;
        }
        model = new;
        flash = post;
    }
    Ok(fixtures)
}

/// Mount `pre` fault-free and run `step`: the post flash, the mount's ops, the
/// step's ops, and the step's error if it failed.
pub fn dry_run(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    pre: &NorFlashSim,
    step: &Step,
) -> Result<(NorFlashSim, u64, u64, Option<StoreError>), String> {
    let mut flash = pre.clone();
    flash.power_cycle(FaultPlan::none());
    let mut store = cand
        .mount(flash, cfg)
        .map_err(|(e, _)| format!("mount (fault-free): {e}"))?;
    let mount_ops = store.flash_snapshot().ops_since_plan();
    let r = run_step(store.as_mut(), step);
    let flash = store.into_flash();
    let step_ops = flash.ops_since_plan() - mount_ops;
    Ok((flash, mount_ops, step_ops, r.err()))
}

/// Run one case, panics caught and scored.
pub fn run_case(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    fx: &StepFixture,
    case: &CutCase,
) -> CaseOutcome {
    match catch_quiet(|| run_case_inner(cand, cfg, fx, case)) {
        Ok(o) => o,
        Err(msg) => CaseOutcome {
            failure: Some(Failure::new("panic", msg)),
            atomic: false,
            landed: true,
            torn_erase: false,
            final_flash: None,
        },
    }
}

fn mount(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    flash: NorFlashSim,
    kind: &str,
) -> Result<Box<dyn CandidateStore>, Failure> {
    cand.mount(flash, cfg)
        .map_err(|(e, _)| Failure::new(kind, format!("mount: {e}")))
}

fn run_case_inner(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    fx: &StepFixture,
    case: &CutCase,
) -> CaseOutcome {
    let mut landed = false;
    let mut torn_erase = false;
    let mut atomic = false;
    let r = (|| -> Result<NorFlashSim, Failure> {
        let tear = case.tear_model();
        let seed = case.seed;
        let mut flash = fx.pre.clone();
        let torn_erases_before = flash.stats().torn_erases;
        flash.set_read_budget(Some(READ_BUDGET));
        flash.set_panic_on_violation(true);
        flash.power_cycle(FaultPlan {
            cut_after: Some(fx.mount_ops + case.cut_after),
            tear,
            seed,
        });
        let mut store = mount(cand, cfg, flash, "setup_mount_failed")?;
        let r = run_step(store.as_mut(), &fx.step);
        let mut flash = store.into_flash();
        landed = !flash.is_powered();
        torn_erase = flash.stats().torn_erases > torn_erases_before;
        if let Err(e) = r
            && !landed
        {
            return Err(Failure::new("step_error_without_cut", e.to_string()));
        }
        if let Some(SecondCut::Mount { at }) = case.second {
            flash.power_cycle(FaultPlan {
                cut_after: Some(at),
                tear,
                seed: seed ^ 0xD0D0,
            });
            flash = match cand.mount(flash, cfg) {
                Ok(s) => s.into_flash(),
                Err((_, f)) => f,
            };
        }
        flash.power_cycle(FaultPlan {
            cut_after: None,
            tear,
            seed: seed ^ 0xC1C1,
        });
        let paths = paths_of(&fx.old, &fx.new);
        let mut store = mount(cand, cfg, flash, "unmountable")?;
        let state = read_state(store.as_mut(), &paths)?;
        atomic = judge_old_or_new(&state, &fx.old, &fx.new)?;
        if let Some(SecondCut::Rerun { at }) = case.second {
            let flash = store.into_flash();
            let s3 = seed ^ 0x3333;
            let mut probe = flash.clone();
            probe.power_cycle(FaultPlan {
                cut_after: None,
                tear,
                seed: s3,
            });
            let m2 = mount(cand, cfg, probe, "unmountable")?
                .flash_snapshot()
                .ops_since_plan();
            let mut flash = flash;
            flash.power_cycle(FaultPlan {
                cut_after: Some(m2 + at),
                tear,
                seed: s3,
            });
            let mut st = mount(cand, cfg, flash, "unmountable")?;
            let _ = run_step(st.as_mut(), &fx.step);
            let mut flash = st.into_flash();
            flash.power_cycle(FaultPlan {
                cut_after: None,
                tear,
                seed: seed ^ 0x4444,
            });
            store = mount(cand, cfg, flash, "unmountable")?;
            let state = read_state(store.as_mut(), &paths)?;
            atomic &= judge_old_or_new(&state, &fx.old, &fx.new)?;
        }
        // Still writable: the interrupted step again, then one more.
        run_step(store.as_mut(), &fx.step)
            .map_err(|e| Failure::new("rerun_failed", e.to_string()))?;
        let state = read_state(store.as_mut(), &paths)?;
        if state != fx.new {
            return Err(Failure::new(
                "rerun_wrong_state",
                first_diff(&state, &fx.new),
            ));
        }
        run_step(store.as_mut(), &fx.next)
            .map_err(|e| Failure::new("next_step_failed", e.to_string()))?;
        let paths2 = paths_of(&fx.new, &fx.after_next);
        let mut flash = store.into_flash();
        flash.power_cycle(FaultPlan::none());
        let mut store = mount(cand, cfg, flash, "remount_failed")?;
        let state = read_state(store.as_mut(), &paths2)?;
        if state != fx.after_next {
            return Err(Failure::new(
                "remount_wrong_state",
                first_diff(&state, &fx.after_next),
            ));
        }
        Ok(store.into_flash())
    })();
    match r {
        Ok(f) => CaseOutcome {
            failure: None,
            atomic,
            landed,
            torn_erase,
            final_flash: Some(f),
        },
        Err(failure) => CaseOutcome {
            failure: Some(failure),
            atomic,
            landed,
            torn_erase,
            final_flash: None,
        },
    }
}

/// For double cuts: the mount's operation count after a first cut at `k`
/// (the mount the second cut lands in), or `None` when the cut flash does not
/// mount fault-free.
pub fn mount_ops_after_cut(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    fx: &StepFixture,
    k: u64,
    tear: TearModel,
    seed: u64,
) -> Option<u64> {
    catch_quiet(|| {
        let mut flash = fx.pre.clone();
        flash.set_read_budget(Some(READ_BUDGET));
        flash.power_cycle(FaultPlan {
            cut_after: Some(fx.mount_ops + k),
            tear,
            seed,
        });
        let mut store = cand.mount(flash, cfg).ok()?;
        let _ = run_step(store.as_mut(), &fx.step);
        let mut flash = store.into_flash();
        flash.power_cycle(FaultPlan {
            cut_after: None,
            tear,
            seed: seed ^ 0xD0D0,
        });
        let store = cand.mount(flash, cfg).ok()?;
        Some(store.flash_snapshot().ops_since_plan())
    })
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};
    use crate::{CorpusSet, WorkloadKind};

    fn fixture(
        layout: MemLayout,
    ) -> (
        MemCandidate,
        CandidateConfig,
        Vec<StepFixture>,
        WorkloadSpec,
    ) {
        let cand = MemCandidate::new(layout);
        let cfg = CandidateConfig::new(16);
        let spec = WorkloadSpec::new(WorkloadKind::Save, "syn:3:300", 1);
        let wl = CorpusSet::new(None).build(&spec).unwrap();
        let fx = prepare_fixtures(&cand, &cfg, &wl).unwrap();
        (cand, cfg, fx, spec)
    }

    fn case(spec: &WorkloadSpec, step: usize, k: u64) -> CutCase {
        CutCase {
            candidate: "mem".into(),
            config: CandidateConfig::new(16),
            workload: spec.clone(),
            step,
            cut_after: k,
            tear: "random_bits".into(),
            seed: 5,
            second: None,
        }
    }

    #[test]
    fn fixtures_count_ops_and_chain_models() {
        let (_, _, fx, _) = fixture(MemLayout::PingPong);
        assert_eq!(fx.len(), 22);
        assert!(fx.iter().all(|f| f.step_ops > 0 && f.dry_error.is_none()));
        assert_eq!(fx[3].old, fx[2].new);
    }

    #[test]
    fn a_correct_store_passes_a_cut_and_a_broken_one_fails_one() {
        let (cand, cfg, fx, spec) = fixture(MemLayout::PingPong);
        for k in 0..=fx[2].step_ops {
            let o = run_case(&cand, &cfg, &fx[2], &case(&spec, 2, k));
            assert_eq!(o.failure, None, "k {k}");
            assert!(o.atomic);
        }
        let (cand, cfg, fx, spec) = fixture(MemLayout::InPlace);
        let failed = (0..=fx[2].step_ops)
            .filter(|&k| {
                run_case(&cand, &cfg, &fx[2], &case(&spec, 2, k))
                    .failure
                    .is_some()
            })
            .count();
        assert!(failed > 0);
    }
}
