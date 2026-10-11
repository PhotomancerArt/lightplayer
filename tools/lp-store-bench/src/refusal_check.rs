//! A step the store refused with no cut (on a full flash, `NoSpace`): power
//! cycle, remount, and check what it holds. Every path must be its old or
//! its new value; a store that calls itself step-atomic (`CandidateReport.
//! step_atomic`, T1) must hold exactly the old state — a refused step never
//! committed. Returns the remounted store and the state it holds (the
//! driver's model from here on).

use lp_nor_sim::{FaultPlan, NorFlashSim};

use crate::oracle::{Failure, Model, first_diff, judge_old_or_new, paths_of, read_state};
use crate::{Candidate, CandidateConfig, CandidateStore};

pub fn check_refusal(
    cand: &dyn Candidate,
    cfg: &CandidateConfig,
    mut flash: NorFlashSim,
    old: &Model,
    new: &Model,
) -> Result<(Box<dyn CandidateStore>, Model), Failure> {
    flash.power_cycle(FaultPlan::none());
    // The store goes on from here: no read watchdog (it counts per power
    // cycle, and a long walk powers this flash up only now and then).
    flash.set_read_budget(None);
    let mut store = cand
        .mount(flash, cfg)
        .map_err(|(e, _)| Failure::new("unmountable_after_refusal", format!("mount: {e}")))?;
    let state = read_state(store.as_mut(), &paths_of(old, new))?;
    judge_old_or_new(&state, old, new)?;
    if store.report().step_atomic && state != *old {
        return Err(Failure::new("refusal_not_old", first_diff(&state, old)));
    }
    Ok((store, state))
}
