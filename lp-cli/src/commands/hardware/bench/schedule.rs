//! The ramp/bisect schedule: which LED count the bench tries next, given
//! everything the previous steps did.
//!
//! Pure and IO-free on purpose. The expensive, unrepeatable part of a bench
//! run is the hardware; the part that decides where the boundary is must be
//! testable without any. Feed it step results, ask it for the next step.
//!
//! The procedure is the one the metric definition pins (`leds.max-safe@1`,
//! see `measurements/README.md`): start at a known-good count, double until
//! something dies, bisect to [`BISECT_RESOLUTION_LEDS`], then require
//! [`BOUNDARY_CONFIRMATIONS`] survivals at the boundary before believing it.
//!
//! There is no mutable phase field. Every decision is derived from the
//! recorded results, which is what makes the awkward case fall out for free:
//! if a *confirmation* run at the boundary dies, that death simply becomes
//! the new upper bound and the bisect resumes below it.

/// How close the bisect has to bring the survive/die bracket before the
/// boundary is considered found. Part of the metric definition.
pub const BISECT_RESOLUTION_LEDS: u32 = 10;

/// Survivals required at the boundary before it is recorded. A boundary that
/// only survives once is a coin flip, not a measurement.
pub const BOUNDARY_CONFIRMATIONS: usize = 2;

/// Ramp ceiling. Well past what any ESP32 the bench targets can address at
/// ~90 bytes of buffer per LED; reaching it means the workload is not
/// exercising memory the way the metric assumes, not that the board is
/// enormous.
pub const MAX_RAMP_LEDS: u32 = 8_192;

/// What one bench step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// The workload loaded, rendered, and was still rendering after the
    /// settle.
    Survived,
    /// The device died and the next boot's recovery ledger named an OOM.
    Died,
}

/// One recorded step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepResult {
    pub leds: u32,
    pub outcome: StepOutcome,
}

/// What the schedule wants to happen next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleStep {
    /// Run the workload at this LED count.
    Test(u32),
    /// The run is finished. `boundary` is the largest count confirmed to
    /// survive; zero means nothing survived at all.
    Done { boundary: u32 },
    /// The ramp passed [`MAX_RAMP_LEDS`] without a death. `survived` is the
    /// largest count that did survive.
    OutOfRange { survived: u32 },
}

/// The ramp/bisect state machine.
#[derive(Debug, Clone)]
pub struct BenchSchedule {
    start: u32,
    results: Vec<StepResult>,
}

impl BenchSchedule {
    /// Start the ramp at `start` LEDs (a known-good count for the chip; see
    /// [`default_start_leds`]). Zero is not a strip, so it is clamped up.
    /// Seed the schedule with an already-known bracket: `floor` survived and
    /// `ceiling` died, both on this board and build. The ramp then goes
    /// straight to bisecting between them.
    ///
    /// The results are recorded as if measured, which is exactly what they
    /// are — just measured by an earlier run. Nothing else in the schedule
    /// needs to know: every decision is derived from the results list.
    pub fn seeded(start: u32, floor: Option<u32>, ceiling: Option<u32>) -> Self {
        let mut schedule = Self::new(start);
        if let Some(floor) = floor {
            schedule.record(floor, StepOutcome::Survived);
        }
        if let Some(ceiling) = ceiling {
            schedule.record(ceiling, StepOutcome::Died);
        }
        schedule
    }

    pub fn new(start: u32) -> Self {
        Self {
            start: start.max(1),
            results: Vec::new(),
        }
    }

    /// Record what a step did.
    pub fn record(&mut self, leds: u32, outcome: StepOutcome) {
        self.results.push(StepResult { leds, outcome });
    }

    /// Every step so far, in the order they ran.
    pub fn results(&self) -> &[StepResult] {
        &self.results
    }

    /// What to do next.
    pub fn next_step(&self) -> ScheduleStep {
        let Some(ceiling) = self.lowest_death() else {
            // Nothing has died yet: keep doubling.
            let next = match self.highest_survivor() {
                Some(survived) => survived.saturating_mul(2),
                None => self.start,
            };
            if next > MAX_RAMP_LEDS {
                return ScheduleStep::OutOfRange {
                    survived: self.highest_survivor().unwrap_or(0),
                };
            }
            return ScheduleStep::Test(next);
        };

        // The bracket: the best survivor strictly below the lowest death.
        // Deriving the floor this way (rather than latching it) is what lets
        // a death at the boundary reopen the bisect — that death becomes the
        // ceiling, and the floor falls back to the survivor below it.
        let floor = self.highest_survivor_below(ceiling).unwrap_or(0);

        if ceiling - floor > BISECT_RESOLUTION_LEDS {
            // `ceiling - floor > 10` keeps the midpoint strictly inside the
            // bracket, so a step is never repeated by the bisect.
            return ScheduleStep::Test(floor + (ceiling - floor) / 2);
        }
        if floor == 0 {
            // Nothing survived, down to the resolution. The caller refuses to
            // record a boundary of zero.
            return ScheduleStep::Done { boundary: 0 };
        }
        // Every result at `floor` is a survival: a death there would have
        // become the ceiling and pushed the floor lower.
        if self.survivals_at(floor) >= BOUNDARY_CONFIRMATIONS {
            ScheduleStep::Done { boundary: floor }
        } else {
            ScheduleStep::Test(floor)
        }
    }

    fn highest_survivor(&self) -> Option<u32> {
        self.survivors().max()
    }

    fn highest_survivor_below(&self, ceiling: u32) -> Option<u32> {
        self.survivors().filter(|leds| *leds < ceiling).max()
    }

    fn survivals_at(&self, leds: u32) -> usize {
        self.survivors()
            .filter(|survived| *survived == leds)
            .count()
    }

    fn survivors(&self) -> impl Iterator<Item = u32> + '_ {
        self.results
            .iter()
            .filter(|result| result.outcome == StepOutcome::Survived)
            .map(|result| result.leds)
    }

    fn lowest_death(&self) -> Option<u32> {
        self.results
            .iter()
            .filter(|result| result.outcome == StepOutcome::Died)
            .map(|result| result.leds)
            .min()
    }
}

/// Where the ramp starts for a chip, per the plan's Q3: the classic ESP32 has
/// the least SRAM and the v3 flash-budget ADR already put its comfortable
/// count near 120; the C6 and S3 start higher because starting low only costs
/// extra doubling steps.
pub fn default_start_leds(chip_name: &str) -> u32 {
    match chip_name {
        "esp32" => 120,
        _ => 200,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a whole run against a model board that survives exactly
    /// `capacity` LEDs, returning the recorded steps and the verdict.
    fn run_to_completion(start: u32, capacity: u32) -> (Vec<StepResult>, ScheduleStep) {
        let mut schedule = BenchSchedule::new(start);
        for _ in 0..64 {
            match schedule.next_step() {
                ScheduleStep::Test(leds) => {
                    let outcome = if leds <= capacity {
                        StepOutcome::Survived
                    } else {
                        StepOutcome::Died
                    };
                    schedule.record(leds, outcome);
                }
                done => return (schedule.results().to_vec(), done),
            }
        }
        panic!("schedule did not terminate within 64 steps (capacity {capacity})");
    }

    #[test]
    fn the_first_step_is_the_start_count() {
        assert_eq!(BenchSchedule::new(200).next_step(), ScheduleStep::Test(200));
    }

    #[test]
    fn survivals_double_until_something_dies() {
        let mut schedule = BenchSchedule::new(120);
        for expected in [120, 240, 480] {
            assert_eq!(schedule.next_step(), ScheduleStep::Test(expected));
            schedule.record(expected, StepOutcome::Survived);
        }
        assert_eq!(schedule.next_step(), ScheduleStep::Test(960));

        // The first death ends the ramp and starts the bisect between the
        // last survivor and the death.
        schedule.record(960, StepOutcome::Died);
        assert_eq!(schedule.next_step(), ScheduleStep::Test(720));
    }

    #[test]
    fn the_bisect_halves_the_bracket_until_it_is_within_the_resolution() {
        let mut schedule = BenchSchedule::new(200);
        schedule.record(200, StepOutcome::Survived);
        schedule.record(400, StepOutcome::Died);

        // 200..400 → 300 → (dies) 200..300 → 250 → (survives) 250..300 → 275
        assert_eq!(schedule.next_step(), ScheduleStep::Test(300));
        schedule.record(300, StepOutcome::Died);
        assert_eq!(schedule.next_step(), ScheduleStep::Test(250));
        schedule.record(250, StepOutcome::Survived);
        assert_eq!(schedule.next_step(), ScheduleStep::Test(275));
        schedule.record(275, StepOutcome::Died);
        assert_eq!(schedule.next_step(), ScheduleStep::Test(262));
        schedule.record(262, StepOutcome::Died);

        // 250..262 is still wider than the resolution by two, so one more.
        assert_eq!(schedule.next_step(), ScheduleStep::Test(256));
        schedule.record(256, StepOutcome::Died);

        // 250..256 is within ±10: confirm the boundary instead of splitting.
        assert_eq!(schedule.next_step(), ScheduleStep::Test(250));
    }

    #[test]
    fn the_boundary_needs_two_survivals_before_it_is_believed() {
        let mut schedule = BenchSchedule::new(200);
        schedule.record(200, StepOutcome::Survived);
        schedule.record(205, StepOutcome::Died);

        // One survival at 200 is not enough.
        assert_eq!(schedule.next_step(), ScheduleStep::Test(200));
        schedule.record(200, StepOutcome::Survived);
        assert_eq!(schedule.next_step(), ScheduleStep::Done { boundary: 200 });
    }

    /// The case a latched "confirmed floor" would get wrong: the boundary
    /// survived once and then died. That death is evidence, so it becomes the
    /// new ceiling and the bisect resumes below it.
    #[test]
    fn a_death_during_confirmation_reopens_the_bisect_below_it() {
        let mut schedule = BenchSchedule::new(100);
        schedule.record(100, StepOutcome::Survived);
        schedule.record(200, StepOutcome::Died);
        schedule.record(150, StepOutcome::Survived);
        schedule.record(155, StepOutcome::Died);

        assert_eq!(schedule.next_step(), ScheduleStep::Test(150));
        schedule.record(150, StepOutcome::Died);

        // 150 is now the ceiling and 100 the floor — back to bisecting.
        assert_eq!(schedule.next_step(), ScheduleStep::Test(125));
    }

    #[test]
    fn a_board_that_dies_at_every_count_finishes_with_no_boundary() {
        let (_, verdict) = run_to_completion(200, 0);
        assert_eq!(verdict, ScheduleStep::Done { boundary: 0 });
    }

    #[test]
    fn a_ramp_that_never_dies_stops_at_the_ceiling() {
        let (results, verdict) = run_to_completion(200, u32::MAX);
        assert_eq!(
            verdict,
            ScheduleStep::OutOfRange { survived: 6_400 },
            "results: {results:?}"
        );
        assert!(
            results.iter().all(|result| result.leds <= MAX_RAMP_LEDS),
            "the ramp must never test past the ceiling: {results:?}"
        );
    }

    /// The property that matters: for any board capacity, the run terminates,
    /// the recorded boundary really survived, and it is within the bisect
    /// resolution of the truth.
    #[test]
    fn every_capacity_lands_within_the_resolution_of_the_truth() {
        for capacity in [
            11, 60, 119, 120, 121, 199, 200, 201, 250, 333, 512, 1_000, 4_097,
        ] {
            for start in [120, 200] {
                let (results, verdict) = run_to_completion(start, capacity);
                let ScheduleStep::Done { boundary } = verdict else {
                    panic!("capacity {capacity} from {start} did not finish: {verdict:?}");
                };

                assert!(
                    boundary <= capacity,
                    "capacity {capacity} from {start}: boundary {boundary} never survived"
                );
                assert!(
                    capacity - boundary <= BISECT_RESOLUTION_LEDS,
                    "capacity {capacity} from {start}: boundary {boundary} is more than \
                     ±{BISECT_RESOLUTION_LEDS} away"
                );
                assert!(
                    results
                        .iter()
                        .filter(|result| result.leds == boundary
                            && result.outcome == StepOutcome::Survived)
                        .count()
                        >= BOUNDARY_CONFIRMATIONS,
                    "capacity {capacity} from {start}: boundary {boundary} was not confirmed \
                     {BOUNDARY_CONFIRMATIONS}×: {results:?}"
                );
            }
        }
    }

    /// A capacity below the start still terminates — the bisect walks down.
    #[test]
    fn a_start_above_the_capacity_bisects_downwards() {
        let (results, verdict) = run_to_completion(200, 60);
        let ScheduleStep::Done { boundary } = verdict else {
            panic!("did not finish: {verdict:?}");
        };
        assert!(
            boundary <= 60 && 60 - boundary <= BISECT_RESOLUTION_LEDS,
            "{boundary}"
        );
        assert_eq!(results[0].leds, 200);
        assert_eq!(results[0].outcome, StepOutcome::Died);
    }

    #[test]
    fn the_start_count_follows_the_chip() {
        assert_eq!(default_start_leds("esp32"), 120);
        assert_eq!(default_start_leds("esp32c6"), 200);
        assert_eq!(default_start_leds("esp32s3"), 200);
    }

    /// A seeded bracket goes straight to bisecting: no re-walking a floor we
    /// already paid for, which is where a re-run's minutes go.
    #[test]
    fn a_seeded_bracket_bisects_immediately() {
        let schedule = BenchSchedule::seeded(480, Some(480), Some(720));
        match schedule.next_step() {
            ScheduleStep::Test(leds) => {
                assert!(
                    leds > 480 && leds < 720,
                    "expected a midpoint inside the seeded bracket, got {leds}"
                );
            }
            other => panic!("expected a bisect step, got {other:?}"),
        }
    }

    /// Seeding is optional and additive: no seeds behaves exactly like `new`.
    #[test]
    fn seeding_nothing_is_a_plain_ramp() {
        assert_eq!(
            BenchSchedule::seeded(200, None, None).next_step(),
            BenchSchedule::new(200).next_step()
        );
    }
}
