//! [`ProvisionalBoardNumbers`]: the small number a `new-<n>` board ref
//! wears while its link or device has not said who it is.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::DeviceId;

/// The numbers `new-<n>` offer paths wear ([`super::BoardRef::New`]).
///
/// The roster's handle ([`DeviceId`]) is no name to read: the ids registry
/// rows without one are loaded under start at `u64::MAX / 2`, and every
/// later mint follows them, so a blank board's flash read
/// `devices/new-9223372036854775809/flash` (activity corpus S18,
/// 2026-10-03). The number is the entry's place among what is unidentified
/// instead:
///
/// - an entry keeps its number for as long as it stays unidentified,
///   whatever comes and goes around it — a path the agent was shown stays
///   good while the link is pending;
/// - a newcomer takes the smallest number nobody holds (newcomers of one
///   build in handle order), so one unidentified board on the desk is
///   `new-1`;
/// - an entry that identifies (its path becomes `mac-…`, `sim-…` or
///   `emu-…`) or goes lets its number go, and only then can a later
///   newcomer wear it.
#[derive(Debug, Default)]
pub struct ProvisionalBoardNumbers {
    held: BTreeMap<DeviceId, u32>,
}

impl ProvisionalBoardNumbers {
    /// Number the entries that are unidentified now: each keeps the number
    /// it holds, the rest take the smallest free ones, and every entry not
    /// in `unidentified` lets its number go.
    pub fn renumber(&mut self, unidentified: &[DeviceId]) {
        let now: BTreeSet<DeviceId> = unidentified.iter().copied().collect();
        self.held.retain(|device, _| now.contains(device));
        let mut taken: BTreeSet<u32> = self.held.values().copied().collect();
        let mut next = 1;
        for device in now {
            if self.held.contains_key(&device) {
                continue;
            }
            while taken.contains(&next) {
                next += 1;
            }
            taken.insert(next);
            self.held.insert(device, next);
        }
    }

    /// The number `device` holds, when it is unidentified.
    pub fn number(&self, device: DeviceId) -> Option<u32> {
        self.held.get(&device).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_unidentified_board_is_new_1_whatever_its_handle() {
        let mut numbers = ProvisionalBoardNumbers::default();
        let high = DeviceId(9_223_372_036_854_775_809);
        numbers.renumber(&[high]);
        assert_eq!(numbers.number(high), Some(1));
    }

    /// A number stays with its entry while it is unidentified, even when an
    /// entry numbered before it identifies; a freed number goes to the next
    /// newcomer, never to an entry that already has one.
    #[test]
    fn a_number_is_kept_while_pending_and_freed_when_it_identifies() {
        let mut numbers = ProvisionalBoardNumbers::default();
        let (a, b, c) = (DeviceId(7), DeviceId(9), DeviceId(12));
        numbers.renumber(&[b, a]);
        assert_eq!((numbers.number(a), numbers.number(b)), (Some(1), Some(2)));
        // `a` identified: `b` keeps 2.
        numbers.renumber(&[b]);
        assert_eq!((numbers.number(a), numbers.number(b)), (None, Some(2)));
        // A newcomer takes the freed 1.
        numbers.renumber(&[b, c]);
        assert_eq!((numbers.number(b), numbers.number(c)), (Some(2), Some(1)));
        // Steady state: nothing moves.
        numbers.renumber(&[c, b]);
        assert_eq!((numbers.number(b), numbers.number(c)), (Some(2), Some(1)));
    }
}
