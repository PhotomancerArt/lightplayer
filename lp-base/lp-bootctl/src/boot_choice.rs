//! Which of the two boot records the loader follows.

use crate::boot_record::BootSlot;

/// The loader's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootChoice {
    /// Which record sector (0 or 1) it came from.
    pub sector: usize,
    pub slot: BootSlot,
    /// The newer record is a trial that failed, and this is the one before.
    pub rolled_back: bool,
}

/// Newest valid record wins; a **failed trial** (ran, never confirmed) gives
/// way to the other record when that one is proven. A failed trial with
/// nothing to fall back to is booted anyway: a board that might work beats
/// one that certainly does not.
pub fn choose(sectors: [Option<BootSlot>; 2]) -> Option<BootChoice> {
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
    if newest_slot.failed() && sectors[other].is_some_and(|s| s.proven()) {
        return pick(other, true);
    }
    pick(newest, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_record::{BootMarks, BootRecord};

    fn slot(seq: u32, trial: bool, attempted: bool, confirmed: bool) -> Option<BootSlot> {
        Some(BootSlot {
            record: BootRecord {
                seq,
                core_off: seq * 0x10000,
                core_len: 1,
                trial,
            },
            marks: BootMarks {
                attempted,
                confirmed,
            },
        })
    }

    #[test]
    fn nothing_valid_is_no_choice() {
        assert_eq!(choose([None, None]), None);
    }

    #[test]
    fn the_newest_wins_in_either_sector() {
        assert_eq!(
            choose([slot(1, false, false, false), slot(2, false, false, false)])
                .unwrap()
                .sector,
            1
        );
        assert_eq!(
            choose([slot(3, false, false, false), slot(2, false, false, false)])
                .unwrap()
                .sector,
            0
        );
    }

    #[test]
    fn a_fresh_trial_is_booted() {
        let c = choose([slot(1, false, false, false), slot(2, true, false, false)]).unwrap();
        assert_eq!((c.sector, c.rolled_back), (1, false));
    }

    #[test]
    fn a_failed_trial_rolls_back_to_the_proven_one() {
        let c = choose([slot(1, false, false, false), slot(2, true, true, false)]).unwrap();
        assert_eq!((c.sector, c.rolled_back), (0, true));
    }

    #[test]
    fn a_confirmed_trial_stays() {
        let c = choose([slot(1, false, false, false), slot(2, true, true, true)]).unwrap();
        assert_eq!((c.sector, c.rolled_back), (1, false));
    }

    #[test]
    fn a_failed_trial_with_nothing_proven_behind_it_is_booted_anyway() {
        let c = choose([None, slot(2, true, true, false)]).unwrap();
        assert_eq!((c.sector, c.rolled_back), (1, false));
        let c = choose([slot(1, true, true, false), slot(2, true, true, false)]).unwrap();
        assert_eq!((c.sector, c.rolled_back), (1, false));
    }
}
