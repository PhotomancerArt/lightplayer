//! The one home of a frame plan, with its rebuild-and-compare checker.

use core::fmt::Debug;

use super::{PlanHost, PlanMode};

/// A plan `T` built under key `K`, replayed while the key holds.
///
/// Every frame plan lives in one of these, so every plan gets the same
/// checker: under [`PlanMode::Check`] a replay also rebuilds the plan from
/// scratch and compares, and a difference — a key that did not move when
/// what the plan holds did — names the owner, the plan's type and the first
/// line where the two plans' `Debug` forms part. That is what makes a cache
/// provably equal to the walk it replaces *without someone first imagining
/// how it goes stale*. A plan kept anywhere else is a review-time defect.
///
/// A plan is rebuildable state: [`Self::clear`] drops it (memory pressure),
/// and the next frame builds it again.
#[derive(Debug)]
pub struct PlanSlot<K, T> {
    held: Option<(K, T)>,
    /// A non-strict check reports a slot's first mismatch only.
    #[cfg(debug_assertions)]
    mismatch_reported: bool,
}

impl<K, T> Default for PlanSlot<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, T> PlanSlot<K, T> {
    pub const fn new() -> Self {
        Self {
            held: None,
            #[cfg(debug_assertions)]
            mismatch_reported: false,
        }
    }

    /// Drop the plan; the next frame builds it again.
    pub fn clear(&mut self) {
        self.held = None;
    }

    /// The plan as last built, whatever its key.
    pub fn get(&self) -> Option<&T> {
        self.held.as_ref().map(|(_, plan)| plan)
    }
}

impl<K: PartialEq + Debug, T: PartialEq + Debug> PlanSlot<K, T> {
    /// The plan for `key`: replayed when the stored key equals it (and the
    /// mode replays), otherwise built by `build` and stored. `build` runs at
    /// most once per call and receives `host` back, so it may resolve
    /// through the node's context.
    pub fn get_or_build<H: PlanHost + ?Sized>(
        &mut self,
        key: K,
        host: &mut H,
        build: impl FnOnce(&mut H) -> T,
    ) -> &T {
        match self.get_or_try_build(key, host, |host| {
            Ok::<T, core::convert::Infallible>(build(host))
        }) {
            Ok(plan) => plan,
            Err(never) => match never {},
        }
    }

    /// [`Self::get_or_build`] for a build that can fail. A failed build
    /// stores nothing (the previous plan is dropped), so the next frame tries
    /// again. A failed *check* rebuild is reported as a mismatch and the
    /// replayed plan is kept.
    pub fn get_or_try_build<H: PlanHost + ?Sized, E: Debug>(
        &mut self,
        key: K,
        host: &mut H,
        build: impl FnOnce(&mut H) -> Result<T, E>,
    ) -> Result<&T, E> {
        let mode = host.plan_state().mode;
        let key_holds = matches!(&self.held, Some((held, _)) if *held == key);
        if !key_holds || mode == PlanMode::RebuildEveryFrame {
            host.plan_state().counters.builds += 1;
            self.held = None;
            let plan = build(host)?;
            return Ok(&self.held.insert((key, plan)).1);
        }
        host.plan_state().counters.replays += 1;
        #[cfg(debug_assertions)]
        if let PlanMode::Check { every, strict } = mode
            && host.plan_state().frame % every.max(1) == 0
        {
            // A check is invisible to every other counter: what the rebuild
            // itself counted (plans built inside it, topology derived) is not
            // this frame's work, only the comparison is.
            let counters = host.plan_state().counters;
            let fresh = build(host);
            host.plan_state().counters = counters;
            host.plan_state().counters.checks += 1;
            self.compare(host, fresh, strict);
        }
        #[cfg(not(debug_assertions))]
        let _ = build;
        Ok(&self.held.as_ref().expect("held under a matching key").1)
    }

    #[cfg(debug_assertions)]
    fn compare<H: PlanHost + ?Sized, E: Debug>(
        &mut self,
        host: &mut H,
        fresh: Result<T, E>,
        strict: bool,
    ) {
        let (key, held) = self.held.as_mut().expect("held under a matching key");
        let difference = match &fresh {
            Ok(plan) if plan == held => return,
            Ok(plan) => first_debug_difference(held, plan),
            Err(err) => alloc::format!("the rebuild failed: {err:?}"),
        };
        host.plan_state().counters.check_mismatches += 1;
        let owner = host.plan_owner();
        let message = alloc::format!(
            "frame plan check: {} held by {} replayed under key {key:?} differs from a fresh \
             build — a key the plan reads did not move when what it holds did \
             (docs/adr/2026-10-24-frame-plan-once-per-edit.md). {difference}",
            core::any::type_name::<T>(),
            match owner {
                Some(node) => alloc::format!("node {node:?}"),
                None => alloc::string::String::from("the engine"),
            },
        );
        if strict {
            panic!("{message}");
        }
        if !self.mismatch_reported {
            self.mismatch_reported = true;
            log::error!("{message}");
        }
        // The fresh plan is the truth; serve it from here on.
        if let Ok(plan) = fresh {
            *held = plan;
        }
    }
}

/// The first line where two values' pretty `Debug` forms part, with its line
/// number: in a derived `Debug`, that line names the field.
#[cfg(debug_assertions)]
fn first_debug_difference<T: Debug>(held: &T, fresh: &T) -> alloc::string::String {
    let held = alloc::format!("{held:#?}");
    let fresh = alloc::format!("{fresh:#?}");
    let mut held_lines = held.lines();
    let mut fresh_lines = fresh.lines();
    let mut line = 1usize;
    loop {
        match (held_lines.next(), fresh_lines.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (None, None) => return alloc::string::String::from("(their Debug forms agree)"),
            (a, b) => {
                return alloc::format!(
                    "First difference at line {line} of their Debug forms:\n  replayed: {}\n  rebuilt:  {}",
                    a.unwrap_or("<end>").trim(),
                    b.unwrap_or("<end>").trim(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_plan::FramePlanState;

    #[derive(Debug, PartialEq)]
    struct Plan {
        name: &'static str,
        lamps: u32,
    }

    fn state(mode: PlanMode) -> FramePlanState {
        FramePlanState {
            mode,
            frame: 1,
            ..FramePlanState::default()
        }
    }

    #[test]
    fn a_held_key_replays_and_a_moved_key_rebuilds() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::Replay);
        let mut builds = 0;
        for key in [1u64, 1, 1, 2, 2] {
            let plan = slot.get_or_build(key, &mut host, |_| {
                builds += 1;
                Plan {
                    name: "a",
                    lamps: key as u32,
                }
            });
            assert_eq!(plan.lamps, key as u32);
        }
        assert_eq!(builds, 2);
        assert_eq!(host.counters.builds, 2);
        assert_eq!(host.counters.replays, 3);
    }

    #[test]
    fn rebuild_every_frame_never_replays() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::RebuildEveryFrame);
        for _ in 0..3 {
            slot.get_or_build(7u64, &mut host, |_| Plan {
                name: "a",
                lamps: 1,
            });
        }
        assert_eq!(host.counters.builds, 3);
        assert_eq!(host.counters.replays, 0);
    }

    #[test]
    fn a_check_that_agrees_counts_and_keeps_the_plan() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::strict_check());
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        assert_eq!(host.counters.checks, 1);
        assert_eq!(host.counters.check_mismatches, 0);
    }

    #[test]
    #[should_panic(expected = "First difference at line 3 of their Debug forms")]
    fn a_stale_plan_under_a_held_key_panics_naming_the_field() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::strict_check());
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        // What the plan holds moved; the key did not.
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 4,
        });
    }

    #[test]
    fn a_lenient_check_counts_the_mismatch_and_serves_the_fresh_plan() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::Check {
            every: 1,
            strict: false,
        });
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        let plan = slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 4,
        });
        assert_eq!(plan.lamps, 4);
        assert_eq!(host.counters.check_mismatches, 1);
    }

    #[test]
    fn checks_run_only_on_their_cadence() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::Check {
            every: 4,
            strict: true,
        });
        for frame in 1..=8u32 {
            host.frame = frame;
            slot.get_or_build(1u64, &mut host, |_| Plan {
                name: "a",
                lamps: 3,
            });
        }
        // Frame 1 built; of frames 2..=8, frames 4 and 8 checked.
        assert_eq!(host.counters.checks, 2);
    }

    #[test]
    fn clear_drops_the_plan_so_the_next_frame_builds() {
        let mut slot = PlanSlot::new();
        let mut host = state(PlanMode::Replay);
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        slot.clear();
        assert!(slot.get().is_none());
        slot.get_or_build(1u64, &mut host, |_| Plan {
            name: "a",
            lamps: 3,
        });
        assert_eq!(host.counters.builds, 2);
    }
}
