//! Model-based random sequences: a seeded walk over pushes, re-pushes, saves,
//! panel writes and deletes across a few project slots, with a random cut
//! every few steps, judged by the oracle at each cut. The walk continues from
//! the recovered flash, so later steps run on post-crash states.

use std::collections::BTreeMap;
use std::sync::Arc;

use lp_nor_sim::{NorFlashSim, SimRng, TearModel};
use serde::{Deserialize, Serialize};

use crate::cut_case::{CutCase, StepFixture, dry_run, probe_step, run_case};
use crate::driver_exhaustive::FailureRecord;
use crate::oracle::{Failure, Model, apply_step};
use crate::workload::{board_step, edit_doc, panel_json, push_step};
use crate::{
    Candidate, CandidateConfig, CorpusSet, Reproducer, Scoreboard, Step, WorkloadKind, WorkloadSpec,
};

/// One walk: replayable from these fields alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RandomParams {
    pub candidate: String,
    pub config: CandidateConfig,
    pub corpora: Vec<String>,
    pub seed: u64,
    pub steps: u64,
    /// A step is cut with probability 1 / `cut_one_in`.
    pub cut_one_in: u64,
    /// Stop after this cut (replay of a failure).
    #[serde(default)]
    pub stop_at_cut: Option<u64>,
    /// Tear models a cut draws from, by name; empty = the three guessed
    /// ones ([`TearModel::ALL`]), drawn exactly as before the field existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tears: Vec<String>,
}

impl RandomParams {
    /// The tear models this walk draws from (unknown names are an error).
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RandomSummary {
    pub driver: String,
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub seed: u64,
    pub steps_run: u64,
    pub steps_no_space: u64,
    pub cuts: u64,
    pub failures: u64,
    pub non_atomic: u64,
    pub kinds: BTreeMap<String, u64>,
    pub first_failure: Option<Failure>,
    pub error: Option<String>,
}

pub(crate) const SLOTS: [&str; 3] = ["a", "b", "c"];

pub fn random_walk(
    cand: &dyn Candidate,
    p: &RandomParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
) -> RandomSummary {
    let mut sum = RandomSummary {
        driver: "random".into(),
        candidate: cand.name().into(),
        config: Some(p.config.clone()),
        seed: p.seed,
        ..Default::default()
    };
    if let Err(e) = walk(cand, p, corpora, sink, &mut sum) {
        sum.error = Some(e);
    }
    if p.stop_at_cut.is_none() {
        sink.write("random_summary", &sum);
    }
    sum
}

fn walk(
    cand: &dyn Candidate,
    p: &RandomParams,
    corpora: &CorpusSet,
    sink: &Scoreboard,
    sum: &mut RandomSummary,
) -> Result<(), String> {
    let cfg = &p.config;
    let tears = p.tear_models()?;
    let mut rng = SimRng::new(p.seed);
    let mut flash = NorFlashSim::new(cfg.geometry());
    flash.set_panic_on_violation(true);
    cand.format(&mut flash, cfg)
        .map_err(|e| format!("format: {e}"))?;
    let mut model = Model::new();
    let mut cut_index = 0u64;
    for i in 0..p.steps {
        let step = if i == 0 {
            board_step()
        } else {
            next_step(&mut rng, &model, corpora, &p.corpora)?
        };
        let pre = flash.clone();
        let (post, mount_ops, step_ops, err) =
            crate::catch_quiet(|| dry_run(cand, cfg, &pre, &step))
                .map_err(|e| format!("dry run panicked at step {i}: {e}"))??;
        sum.steps_run += 1;
        if err.is_some() {
            sum.steps_no_space += 1;
            continue;
        }
        let mut new = model.clone();
        apply_step(&mut new, &step);
        if rng.chance(1, p.cut_one_in.max(1)) {
            let next = probe_step(i);
            let mut after_next = new.clone();
            apply_step(&mut after_next, &next);
            let fx = StepFixture {
                index: i as usize,
                pre,
                old: model.clone(),
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
                workload: WorkloadSpec::new(WorkloadKind::Push, "random", p.seed),
                step: i as usize,
                cut_after: rng.below(step_ops + 1),
                tear: tear.name().into(),
                seed: rng.next_u64(),
                second: None,
            };
            let o = run_case(cand, cfg, &fx, &case);
            sum.cuts += 1;
            if let Some(f) = o.failure {
                sum.failures += 1;
                *sum.kinds.entry(f.kind.clone()).or_default() += 1;
                if sum.first_failure.is_none() && p.stop_at_cut.is_none_or(|stop| stop == cut_index)
                {
                    sum.first_failure = Some(f.clone());
                }
                if p.stop_at_cut.is_none() && sum.failures <= 5 {
                    sink.write(
                        "failure",
                        &FailureRecord {
                            driver: "random".into(),
                            failure: f,
                            reproducer: Reproducer::Random(RandomParams {
                                stop_at_cut: Some(cut_index),
                                ..p.clone()
                            }),
                        },
                    );
                }
                flash = post;
                model = new;
            } else {
                sum.non_atomic += !o.atomic as u64;
                flash = o.final_flash.expect("a passing case returns its flash");
                model = after_next;
            }
            if p.stop_at_cut == Some(cut_index) {
                return Ok(());
            }
            cut_index += 1;
        } else {
            flash = post;
            model = new;
        }
    }
    Ok(())
}

fn slots_present(model: &Model) -> Vec<&'static str> {
    SLOTS
        .into_iter()
        .filter(|s| {
            let pre = format!("/projects/{s}/");
            model.keys().any(|p| p.starts_with(&pre))
        })
        .collect()
}

pub(crate) fn next_step(
    rng: &mut SimRng,
    model: &Model,
    corpora: &CorpusSet,
    names: &[String],
) -> Result<Step, String> {
    let present = slots_present(model);
    let roll = rng.below(12);
    if present.is_empty() || roll < 2 {
        let c = corpora.get(&names[rng.below(names.len() as u64) as usize])?;
        let slot = SLOTS[rng.below(SLOTS.len() as u64) as usize];
        let mut s = push_step(&format!("push-{slot}"), slot, &c, &BTreeMap::new());
        s.ops
            .insert(0, crate::Op::DeletePrefix(format!("/projects/{slot}/")));
        return Ok(s);
    }
    let slot = present[rng.below(present.len() as u64) as usize];
    let pre = format!("/projects/{slot}/");
    let docs: Vec<&String> = model.keys().filter(|p| p.starts_with(&pre)).collect();
    Ok(match roll {
        2..=5 => {
            let mut s = Step::new("save");
            let k = 1 + rng.below(3) as usize;
            for i in crate::workload::pick_distinct(rng, docs.len(), k) {
                let d = docs[i];
                s.put(d.clone(), Arc::new(edit_doc(d, &model[d], rng)));
            }
            s
        }
        6..=9 => {
            let mut s = Step::new("panel");
            s.put(format!("{pre}.lp/panel.json"), Arc::new(panel_json(rng)));
            s
        }
        10 => {
            let mut s = Step::new("delete");
            s.delete_prefix(pre);
            s
        }
        _ => {
            let mut s = Step::new("repush");
            s.delete_prefix(pre.clone());
            for d in &docs {
                let b = if d.ends_with(".glsl") && rng.chance(1, 4) {
                    Arc::new(edit_doc(d, &model[*d], rng))
                } else {
                    model[*d].clone()
                };
                s.put((*d).clone(), b);
            }
            s
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};

    fn params(cand: &str) -> RandomParams {
        RandomParams {
            candidate: cand.into(),
            config: CandidateConfig::new(16),
            corpora: vec!["syn:2:200".into(), "syn:3:300".into()],
            seed: 11,
            steps: 40,
            cut_one_in: 3,
            stop_at_cut: None,
            tears: vec![],
        }
    }

    #[test]
    fn random_walk_passes_the_reference_and_catches_the_twin() {
        let set = CorpusSet::new(None);
        let sink = Scoreboard::memory();
        let good = random_walk(
            &MemCandidate::new(MemLayout::PingPong),
            &params("mem"),
            &set,
            &sink,
        );
        assert_eq!(good.failures, 0, "{good:?}");
        assert!(good.cuts > 3);
        let bad = random_walk(
            &MemCandidate::new(MemLayout::InPlace),
            &params("mem-broken"),
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
