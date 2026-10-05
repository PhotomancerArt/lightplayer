//! Which of the two boot records the loader follows.

use crate::boot_record::{BootSlot, COLD_RETRY_CAP};
use crate::reset_kind::ResetKind;

/// The loader's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootChoice {
    /// Which record sector (0 or 1) it came from.
    pub sector: usize,
    pub slot: BootSlot,
    /// The newer record is a trial that failed, and this is the one before.
    pub rolled_back: bool,
}

/// Newest valid record wins, except that a **failed** newest trial gives way
/// to the other record when that one is proven. A failed trial with nothing
/// proven behind it is booted anyway: a board that might work beats one
/// that certainly does not.
///
/// What "failed" means depends on the reset before this boot
/// ([`ResetKind`]):
///
/// - **warm** (the chip reset itself — a panic, a watchdog): the trial ran
///   and never confirmed: `attempted && !confirmed`.
/// - **cold** (the power went, or a host reset the board): the trial kept
///   dying before it finished starting, across [`COLD_RETRY_CAP`] counted
///   retries: `attempted && !started && !confirmed && tally >= cap`. A
///   power cut in the first second of a new core is a retry, not a failure
///   — but a core that looks like a brownout (it dies, cold, every time it
///   brings its radios up) is not retried for ever.
///
/// A trial that **started** and has not confirmed is never failed by a cold
/// boot: nobody has connected to it yet, and power-cycling a board must not
/// roll a good build back. A warm death after it started still fails it.
pub fn choose(sectors: [Option<BootSlot>; 2], reset: ResetKind) -> Option<BootChoice> {
    let pick = |sector: usize, rolled_back| {
        sectors[sector].map(|slot| BootChoice {
            sector,
            slot,
            rolled_back,
        })
    };
    let newest = match sectors {
        [Some(a), Some(b)] => usize::from(b.record.seq > a.record.seq),
        [Some(_), None] => 0,
        [None, Some(_)] => 1,
        [None, None] => return None,
    };
    let other = 1 - newest;
    let newest_slot = sectors[newest]?;
    if failed(&newest_slot, reset) && sectors[other].is_some_and(|s| s.proven()) {
        return pick(other, true);
    }
    pick(newest, false)
}

/// Whether `slot` is a trial that failed, given the reset before this boot.
pub fn failed(slot: &BootSlot, reset: ResetKind) -> bool {
    let m = slot.marks;
    if !slot.record.trial || !m.attempted || m.confirmed {
        return false;
    }
    match reset {
        ResetKind::Warm => true,
        ResetKind::Cold => !m.started && m.cold_retries() >= COLD_RETRY_CAP,
    }
}

/// Whether this boot counts as one more cold retry of `slot`: a cold boot
/// after a trial that ran and never finished starting. The core programs
/// one tally bit when it holds (`BootMarks::next_cold_tally_word`).
pub fn cold_retry_to_count(slot: &BootSlot, reset: ResetKind) -> bool {
    let m = slot.marks;
    reset.is_cold() && slot.record.trial && m.attempted && !m.started && !m.confirmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_record::{BootMarks, BootRecord};

    use ResetKind::{Cold, Warm};

    #[derive(Clone, Copy)]
    struct Marks {
        attempted: bool,
        confirmed: bool,
        started: bool,
        tally: u32,
    }

    const NONE: Marks = Marks {
        attempted: false,
        confirmed: false,
        started: false,
        tally: 0,
    };

    fn slot(seq: u32, trial: bool, m: Marks) -> Option<BootSlot> {
        Some(BootSlot {
            record: BootRecord {
                seq,
                core_off: seq * 0x10000,
                core_len: 1,
                build: seq,
                trial,
            },
            marks: BootMarks {
                attempted: m.attempted,
                confirmed: m.confirmed,
                started: m.started,
                // `tally` counted retries: that many low bits cleared.
                cold_tally: u32::MAX.checked_shl(m.tally).unwrap_or(0),
            },
        })
    }

    fn proven(seq: u32) -> Option<BootSlot> {
        slot(seq, false, NONE)
    }

    fn trial(seq: u32, m: Marks) -> Option<BootSlot> {
        slot(seq, true, m)
    }

    fn attempted() -> Marks {
        Marks {
            attempted: true,
            ..NONE
        }
    }

    fn decision(sectors: [Option<BootSlot>; 2], reset: ResetKind) -> (usize, bool) {
        let c = choose(sectors, reset).unwrap();
        (c.sector, c.rolled_back)
    }

    #[test]
    fn the_whole_truth_table() {
        // Every combination of the trial's marks × the tally × the reset ×
        // whether the record behind it is proven, against the rule written
        // out longhand.
        for attempted in [false, true] {
            for confirmed in [false, true] {
                for started in [false, true] {
                    for tally in 0..=4 {
                        for reset in [Cold, Warm] {
                            for other_proven in [false, true] {
                                let m = Marks {
                                    attempted,
                                    confirmed,
                                    started,
                                    tally,
                                };
                                let other = slot(1, !other_proven, NONE);
                                let c = choose([other, trial(2, m)], reset).unwrap();
                                let failed = attempted
                                    && !confirmed
                                    && match reset {
                                        Warm => true,
                                        Cold => !started && tally >= COLD_RETRY_CAP,
                                    };
                                let expect = if failed && other_proven {
                                    (0, true)
                                } else {
                                    (1, false)
                                };
                                assert_eq!(
                                    (c.sector, c.rolled_back),
                                    expect,
                                    "attempted {attempted} confirmed {confirmed} started \
                                     {started} tally {tally} {reset:?} other proven {other_proven}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_proven_newest_record_is_never_a_failure() {
        for reset in [Cold, Warm] {
            assert_eq!(decision([proven(1), proven(2)], reset), (1, false));
        }
    }

    #[test]
    fn nothing_valid_is_no_choice() {
        assert_eq!(choose([None, None], Warm), None);
    }

    #[test]
    fn the_newest_wins_in_either_sector() {
        assert_eq!(decision([proven(1), proven(2)], Warm).0, 1);
        assert_eq!(decision([proven(3), proven(2)], Warm).0, 0);
    }

    #[test]
    fn a_fresh_trial_is_booted() {
        assert_eq!(decision([proven(1), trial(2, NONE)], Warm), (1, false));
        assert_eq!(decision([proven(1), trial(2, NONE)], Cold), (1, false));
    }

    // E3: a new core crashes on its first boot → back to the old core.
    #[test]
    fn e3_a_new_core_that_crashes_rolls_back() {
        assert_eq!(
            decision([proven(1), trial(2, attempted())], Warm),
            (0, true)
        );
    }

    // E3, after it started: a warm death is still a failure.
    #[test]
    fn e3_a_warm_death_after_starting_still_rolls_back() {
        let m = Marks {
            started: true,
            ..attempted()
        };
        assert_eq!(decision([proven(1), trial(2, m)], Warm), (0, true));
    }

    // E4: a power cut during that first boot → the new core is retried.
    #[test]
    fn e4_a_power_cut_in_the_first_boot_retries_the_new_core() {
        for tally in 0..COLD_RETRY_CAP {
            let m = Marks {
                tally,
                ..attempted()
            };
            assert_eq!(
                decision([proven(1), trial(2, m)], Cold),
                (1, false),
                "{tally}"
            );
        }
    }

    // E11: a new core that looks like a brownout keeps dying cold before it
    // starts → retried up to the cap, then rolled back.
    #[test]
    fn e11_a_brownout_lookalike_is_capped_then_rolled_back() {
        let m = Marks {
            tally: COLD_RETRY_CAP,
            ..attempted()
        };
        assert_eq!(decision([proven(1), trial(2, m)], Cold), (0, true));
    }

    // D9: a trial that started but nobody has connected to is never rolled
    // back by power cycling, however many times.
    #[test]
    fn a_started_unconfirmed_trial_survives_any_number_of_power_cycles() {
        for tally in 0..=32 {
            let m = Marks {
                started: true,
                tally,
                ..attempted()
            };
            assert_eq!(decision([proven(1), trial(2, m)], Cold), (1, false));
        }
    }

    #[test]
    fn a_confirmed_trial_stays() {
        let m = Marks {
            confirmed: true,
            ..attempted()
        };
        for reset in [Cold, Warm] {
            assert_eq!(decision([proven(1), trial(2, m)], reset), (1, false));
        }
    }

    #[test]
    fn a_failed_trial_with_nothing_proven_behind_it_is_booted_anyway() {
        assert_eq!(decision([None, trial(2, attempted())], Warm), (1, false));
        assert_eq!(
            decision([trial(1, attempted()), trial(2, attempted())], Warm),
            (1, false)
        );
    }

    #[test]
    fn a_cold_retry_is_counted_only_for_an_attempted_unstarted_trial() {
        let s = |trial, m| slot(2, trial, m).unwrap();
        assert!(cold_retry_to_count(&s(true, attempted()), Cold));
        assert!(!cold_retry_to_count(&s(true, attempted()), Warm));
        assert!(!cold_retry_to_count(&s(true, NONE), Cold));
        assert!(!cold_retry_to_count(&s(false, attempted()), Cold));
        let started = Marks {
            started: true,
            ..attempted()
        };
        assert!(!cold_retry_to_count(&s(true, started), Cold));
        let confirmed = Marks {
            confirmed: true,
            ..attempted()
        };
        assert!(!cold_retry_to_count(&s(true, confirmed), Cold));
    }

    #[test]
    fn the_cap_is_reached_by_counting_retries() {
        // A brownout lookalike, boot by boot: each cold boot the loader
        // decides on the tally so far, then the core counts that boot.
        let mut marks = BootMarks {
            attempted: true,
            ..BootMarks::default()
        };
        let mut boots = 0;
        loop {
            boots += 1;
            let s = BootSlot {
                record: BootRecord {
                    seq: 2,
                    core_off: 0,
                    core_len: 1,
                    build: 2,
                    trial: true,
                },
                marks,
            };
            if choose([proven(1), Some(s)], Cold).unwrap().rolled_back {
                break;
            }
            assert!(cold_retry_to_count(&s, Cold));
            marks.cold_tally &= marks.next_cold_tally_word().unwrap();
            assert!(boots < 10);
        }
        // Three counted retries after the first death, then the rollback.
        assert_eq!(boots, COLD_RETRY_CAP + 1);
    }
}
