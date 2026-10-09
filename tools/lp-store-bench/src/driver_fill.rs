//! Fill to full: how many copies of a project a store holds (then whether it
//! still saves), and the largest single project it holds and keeps editable.

use std::collections::BTreeMap;
use std::sync::Arc;

use lp_nor_sim::SimRng;
use serde::{Deserialize, Serialize};

use crate::driver_measure::measure_steps;
use crate::workload::{board_step, edit_doc, push_step};
use crate::{Candidate, CandidateConfig, Corpus, CorpusDoc, Step};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FillResult {
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub corpus: String,
    /// Copies of the corpus pushed (each into its own slot) before `NoSpace`.
    pub slots_held: u32,
    /// Of 20 saves on the last slot, how many succeeded.
    pub saves_ok: u32,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LargestResult {
    pub candidate: String,
    pub config: Option<CandidateConfig>,
    pub base: String,
    /// Extra module copies on top of the base corpus (each doc edited, so no
    /// copy is byte-identical to another).
    pub extra_modules: Option<u32>,
    pub modules: Option<u32>,
    pub logical_bytes: Option<u64>,
}

/// Push `corpus` into slots s0, s1, … until `NoSpace` (at most `max_slots`),
/// then try 20 saves on the last slot that landed.
pub fn fill_slots(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    corpus: &Corpus,
    max_slots: u32,
) -> FillResult {
    let mut r = FillResult {
        candidate: cand.name().into(),
        config: Some(cfg.clone()),
        corpus: corpus.name.clone(),
        ..Default::default()
    };
    // Grow the slot count until a push fails.
    let mut held = 0;
    for n in 1..=max_slots {
        let steps = slot_steps(corpus, n, 0);
        let m = measure_steps(cand, cfg, None, steps.into_iter());
        if !m.ok {
            if m.error.as_deref() != Some("NoSpace") {
                r.error = m.error;
            }
            break;
        }
        held = n;
    }
    r.slots_held = held;
    if held > 0 {
        for saves in (0..=20).rev() {
            let m = measure_steps(cand, cfg, None, slot_steps(corpus, held, saves).into_iter());
            if m.ok {
                r.saves_ok = saves;
                break;
            }
        }
    }
    r
}

fn slot_steps(corpus: &Corpus, slots: u32, saves: u32) -> Vec<Step> {
    let mut steps = vec![board_step()];
    for s in 0..slots {
        steps.push(push_step(
            &format!("push-s{s}"),
            &format!("s{s}"),
            corpus,
            &BTreeMap::new(),
        ));
    }
    let mut rng = SimRng::new(77);
    let last = format!("/projects/s{}/", slots - 1);
    let mut cur: BTreeMap<String, Arc<Vec<u8>>> = corpus
        .docs
        .iter()
        .map(|d| (format!("{last}{}", d.rel), d.bytes.clone()))
        .collect();
    let docs: Vec<String> = cur.keys().cloned().collect();
    for i in 0..saves {
        let mut st = Step::new(format!("save-{i}"));
        let p = &docs[rng.below(docs.len() as u64) as usize];
        let b = Arc::new(edit_doc(p, &cur[p], &mut rng));
        cur.insert(p.clone(), b.clone());
        st.put(p.clone(), b);
        steps.push(st);
    }
    steps
}

/// `base` plus `extra` edited copies of its modules (`modules/<m>_x<i>/…`).
pub fn scaled_corpus(base: &Corpus, extra: u32) -> Corpus {
    let mut rng = SimRng::new(0x5CA1E);
    let modules: Vec<String> = {
        let mut m: Vec<String> = base
            .docs
            .iter()
            .filter_map(|d| d.rel.strip_prefix("modules/"))
            .filter_map(|r| r.split_once('/').map(|(m, _)| m.to_string()))
            .collect();
        m.dedup();
        m
    };
    let mut docs = base.docs.clone();
    for i in 0..extra as usize {
        let m = &modules[i % modules.len().max(1)];
        let pre = format!("modules/{m}/");
        for d in base.docs.iter().filter(|d| d.rel.starts_with(&pre)) {
            let rest = &d.rel[pre.len()..];
            let rel = format!("modules/{m}_x{i}/{rest}");
            docs.push(CorpusDoc {
                bytes: Arc::new(edit_doc(&rel, &d.bytes, &mut rng)),
                rel,
            });
        }
    }
    docs.sort_by(|a, b| a.rel.cmp(&b.rel));
    Corpus {
        name: format!("{}+{extra}", base.name),
        docs,
    }
}

/// The most extra modules (binary search in `0..=max_extra`) with which the
/// scaled corpus pushes and then takes 5 saves.
pub fn largest_project(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    base: &Corpus,
    max_extra: u32,
) -> LargestResult {
    let fits = |extra: u32| {
        let c = scaled_corpus(base, extra);
        let mut steps = slot_steps(&c, 1, 5);
        steps.truncate(2 + 5);
        measure_steps(cand, cfg, None, steps.into_iter()).ok
    };
    let mut r = LargestResult {
        candidate: cand.name().into(),
        config: Some(cfg.clone()),
        base: base.name.clone(),
        ..Default::default()
    };
    if !fits(0) {
        return r;
    }
    let (mut lo, mut hi) = (0, max_extra);
    if fits(hi) {
        lo = hi;
    }
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let c = scaled_corpus(base, lo);
    let count = c
        .docs
        .iter()
        .filter(|d| d.rel.starts_with("modules/") && d.rel.ends_with("/module.json"))
        .count();
    r.extra_modules = Some(lo);
    r.modules = Some(count as u32);
    r.logical_bytes = Some(c.total_bytes() as u64);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};

    #[test]
    fn fill_and_largest_on_the_reference() {
        let cand = MemCandidate::new(MemLayout::PingPong);
        let cfg = CandidateConfig::new(8);
        let c = Corpus::synthetic("f", 3, 400, 1);
        let f = fill_slots(&cand, &cfg, &c, 20);
        assert!(f.slots_held >= 1 && f.slots_held < 20, "{f:?}");
        let l = largest_project(&cand, &cfg, &c, 64);
        assert!(l.extra_modules.is_some(), "{l:?}");
        assert_eq!(scaled_corpus(&c, 2).docs.len(), c.docs.len() + 6);
    }
}
