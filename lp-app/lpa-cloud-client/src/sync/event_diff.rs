//! Which events one log has that another does not.

use alloc::vec::Vec;

use lpc_history::HistoryEvent;

/// The events in `have` that `other` does not have, in `have`'s order.
///
/// A multiset difference, not a set difference: two events can be genuinely
/// equal (same instant, same kind) and both belong in the log, so a
/// set-flavored diff would silently drop the second one. Each event in
/// `have` consumes at most one match in `other`.
///
/// This is how the engine decides what to send and what to adopt, and it is
/// deliberately *not* a sequence-number comparison. The service's log is the
/// interleaving of every client's line (see `lp-cloud-domain`'s
/// `push_validation`), so positions do not correspond across sides — only
/// the events themselves do. Events are content-addressed in spirit: the
/// same save recorded twice is the same event, whoever pushed it.
pub(crate) fn events_missing_from(
    have: &[HistoryEvent],
    other: &[HistoryEvent],
) -> Vec<HistoryEvent> {
    let mut matched = alloc::vec![false; other.len()];
    let mut missing = Vec::new();
    for event in have {
        let found = other
            .iter()
            .enumerate()
            .position(|(index, candidate)| !matched[index] && candidate == event);
        match found {
            Some(index) => matched[index] = true,
            None => missing.push(event.clone()),
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{ContentHash, EventKind};

    #[test]
    fn a_suffix_is_the_difference() {
        let log = [created(), saved(b"v1", 2.0), saved(b"v2", 3.0)];
        assert_eq!(events_missing_from(&log, &log[..1]), log[1..].to_vec());
        assert!(events_missing_from(&log[..1], &log).is_empty());
    }

    #[test]
    fn identical_events_are_counted_not_deduplicated() {
        let twice = [saved(b"v1", 2.0), saved(b"v1", 2.0)];
        let once = [saved(b"v1", 2.0)];
        assert_eq!(events_missing_from(&twice, &once), once.to_vec());
        assert!(events_missing_from(&once, &twice).is_empty());
    }

    /// The interleaved case: the service's log carries somebody else's
    /// events between ours, and our unpushed tail still comes out whole.
    #[test]
    fn interleaving_does_not_hide_our_own_tail() {
        let ours = [created(), saved(b"v1", 2.0), saved(b"mine", 4.0)];
        let service = [created(), saved(b"v1", 2.0), saved(b"theirs", 3.0)];
        assert_eq!(
            events_missing_from(&ours, &service),
            alloc::vec![saved(b"mine", 4.0)]
        );
        assert_eq!(
            events_missing_from(&service, &ours),
            alloc::vec![saved(b"theirs", 3.0)]
        );
    }

    fn created() -> HistoryEvent {
        HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        }
    }

    fn saved(name: &[u8], at: f64) -> HistoryEvent {
        HistoryEvent {
            at,
            kind: EventKind::Saved {
                version: ContentHash::of(name),
            },
        }
    }
}
