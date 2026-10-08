//! Endurance: simulated days of use — every day one re-push, ten saves and
//! four hours of panel writes every 10 s — measuring erase spread, write
//! amplification and (self-reported) GC work. Fault-free.

use std::collections::BTreeMap;
use std::sync::Arc;

use lp_nor_sim::SimRng;

use crate::driver_measure::{MeasureResult, measure_steps};
use crate::workload::{PANEL_PATH, board_step, edit_doc, panel_json, push_step};
use crate::{Candidate, CandidateConfig, Corpus, Step};

/// The shape of a simulated day.
#[derive(Clone, Copy, Debug)]
pub struct DayShape {
    pub pushes: u32,
    pub saves: u32,
    /// Panel writes (4 h every 10 s = 1440).
    pub panel_writes: u32,
}

impl Default for DayShape {
    fn default() -> Self {
        Self {
            pushes: 1,
            saves: 10,
            panel_writes: 1440,
        }
    }
}

/// Run `days` days of `shape` on `corpus` (pushed as project `a`).
pub fn endurance(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    corpus: &Corpus,
    days: u32,
    shape: DayShape,
    seed: u64,
) -> MeasureResult {
    measure_steps(cand, cfg, None, day_steps(corpus, days, shape, seed))
}

/// The lazy step stream: board, first push, then each day's steps.
pub fn day_steps(
    corpus: &Corpus,
    days: u32,
    shape: DayShape,
    seed: u64,
) -> impl Iterator<Item = Step> {
    let corpus = corpus.clone();
    let mut rng = SimRng::new(seed);
    let mut cur: BTreeMap<String, Arc<Vec<u8>>> = corpus
        .docs
        .iter()
        .map(|d| (d.rel.clone(), d.bytes.clone()))
        .collect();
    let mut first = vec![board_step(), {
        let mut p = push_step("push", "a", &corpus, &BTreeMap::new());
        p.put(PANEL_PATH, Arc::new(panel_json(&mut rng)));
        p
    }];
    first.reverse();
    let per_day = shape.pushes + shape.saves + shape.panel_writes;
    let mut i = 0u64;
    std::iter::from_fn(move || {
        if let Some(s) = first.pop() {
            return Some(s);
        }
        if i >= days as u64 * per_day as u64 {
            return None;
        }
        let in_day = (i % per_day as u64) as u32;
        i += 1;
        // Spread saves and pushes through the day's panel writes.
        let step = if in_day < shape.pushes {
            let shaders: Vec<String> = cur
                .keys()
                .filter(|p| p.ends_with(".glsl"))
                .cloned()
                .collect();
            for _ in 0..3.min(shaders.len()) {
                let p = &shaders[rng.below(shaders.len() as u64) as usize];
                let b = Arc::new(edit_doc(p, &cur[p], &mut rng));
                cur.insert(p.clone(), b);
            }
            let mut s = Step::new("day-push");
            s.delete_prefix("/projects/a/");
            for (rel, b) in &cur {
                s.put(format!("/projects/a/{rel}"), b.clone());
            }
            s.put(PANEL_PATH, Arc::new(panel_json(&mut rng)));
            s
        } else if in_day < shape.pushes + shape.saves {
            let docs: Vec<String> = cur.keys().cloned().collect();
            let mut s = Step::new("day-save");
            for _ in 0..1 + rng.below(3) {
                let p = &docs[rng.below(docs.len() as u64) as usize];
                let b = Arc::new(edit_doc(p, &cur[p], &mut rng));
                cur.insert(p.clone(), b.clone());
                s.put(format!("/projects/a/{p}"), b);
            }
            s
        } else {
            let mut s = Step::new("day-panel");
            s.put(PANEL_PATH, Arc::new(panel_json(&mut rng)));
            s
        };
        Some(step)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{MemCandidate, MemLayout};

    #[test]
    fn a_short_endurance_run_counts_erases() {
        let c = Corpus::synthetic("e", 3, 300, 1);
        let shape = DayShape {
            pushes: 1,
            saves: 2,
            panel_writes: 5,
        };
        assert_eq!(day_steps(&c, 2, shape, 1).count(), 2 + 2 * 8);
        let m = endurance(
            &MemCandidate::new(MemLayout::PingPong),
            &CandidateConfig::new(16),
            &c,
            2,
            shape,
            1,
        );
        assert!(m.ok, "{m:?}");
        assert!(m.erases_max > 0);
    }
}
