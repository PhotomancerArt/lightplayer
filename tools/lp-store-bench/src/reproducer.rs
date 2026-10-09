//! Reproducers: what a failure record carries so `lp-store-bench replay` can
//! run exactly that case again.

use serde::{Deserialize, Serialize};

use crate::candidates::parse_candidate_spec;
use crate::cut_case::{CutCase, prepare_fixtures, run_case};
use crate::driver_long::{LongParams, long_walk};
use crate::driver_random::{RandomParams, random_walk};
use crate::oracle::Failure;
use crate::{CorpusSet, Scoreboard};

/// A replayable case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reproducer {
    /// A sweep case: rebuild the workload, run its prefix, run the case.
    Case(CutCase),
    /// A random walk: run it again up to its `stop_at_cut`-th cut.
    Random(RandomParams),
    /// A long walk: run it again to its first failure.
    Long(LongParams),
}

/// Replay `r`; `Ok(Some(failure))` when it fails again, `Ok(None)` when it now
/// passes.
pub fn replay(r: &Reproducer, corpora: &CorpusSet) -> Result<Option<Failure>, String> {
    match r {
        Reproducer::Case(case) => {
            let (cand, _) = parse_candidate_spec(&case.candidate, case.config.sectors)?;
            let wl = corpora.build(&case.workload)?;
            let fx = prepare_fixtures(cand.as_ref(), &case.config, &wl)?;
            let f = fx
                .get(case.step)
                .ok_or_else(|| format!("step {} not reached", case.step))?;
            Ok(run_case(cand.as_ref(), &case.config, f, case).failure)
        }
        Reproducer::Random(p) => {
            let (cand, _) = parse_candidate_spec(&p.candidate, p.config.sectors)?;
            let sink = Scoreboard::memory();
            let s = random_walk(cand.as_ref(), p, corpora, &sink);
            Ok(s.first_failure)
        }
        Reproducer::Long(p) => {
            let (cand, _) = parse_candidate_spec(&p.candidate, p.config.sectors)?;
            let s = long_walk(cand.as_ref(), p, corpora, &Scoreboard::memory());
            Ok(s.first_failure)
        }
    }
}
