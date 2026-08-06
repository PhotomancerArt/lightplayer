//! What the server checks about a pushed event batch — and what it
//! deliberately does not.
//!
//! # The problem
//!
//! Push is never blocked for divergence (D5), but a malformed batch must
//! still be refused or the log rots. Those two rules pull against each
//! other, because the server's log is **the interleaving of every client's
//! line**, not any one client's history. When a second head exists, the
//! events describing it were authored against the *pusher's* base, and
//! replaying them onto the interleaved log can legitimately fail — a
//! `Joined` from one line does not straddle the head of the interleaving.
//! Refusing that push would block a divergence, which is exactly the thing
//! D5 forbids.
//!
//! # The rule
//!
//! Validation is two-tier, and which tier a push landed in is reported by
//! [`PushValidation`]:
//!
//! 1. **The first push must replay.** With an empty log there is no other
//!    line to blame, so `stored ++ incoming` must satisfy
//!    [`ProjectHistory::from_events`] outright: origin first, exactly one
//!    origin, joins straddling the head. A failure here is malformed, full
//!    stop.
//! 2. **Later pushes replay if they can.** `stored ++ incoming` is tried;
//!    success means the pusher continued the server's line
//!    ([`PushValidation::Linear`]). Failure is *not* an error — it is the
//!    signature of a divergent line — so the batch is instead checked for
//!    internal consistency ([`PushValidation::Divergent`]): finite
//!    timestamps, no origin event (a project has exactly one, and it is
//!    already stored), and joins that name two distinct versions.
//!
//! Anything beyond that would need the pusher's own base, which the server
//! does not have and — being content-opaque (D3) — has no way to
//! reconstruct. Saying so plainly is better than a check that looks
//! rigorous and is really a coin flip.

use alloc::format;
use alloc::vec::Vec;
use lpc_cloud_api::CloudError;
use lpc_history::{EventKind, HistoryEvent, ProjectHistory};

use crate::model::stored_event::StoredEvent;

/// How a pushed batch related to the server's stored log.
///
/// Informational — both variants are accepted. It exists so callers and
/// tests can tell "continued the line" from "recorded a divergence" without
/// re-deriving it from the frontier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushValidation {
    /// The batch replays cleanly onto the stored log.
    Linear,
    /// The batch describes a line of its own. Internally consistent, and
    /// accepted as a second head.
    Divergent,
}

/// Validate a pushed event batch against a project's stored log.
///
/// See the module docs for why a replay failure on a non-empty log is a
/// divergence rather than a rejection.
pub fn validate_push_events(
    stored: &[StoredEvent],
    incoming: &[HistoryEvent],
) -> Result<PushValidation, CloudError> {
    if incoming.is_empty() {
        return Err(invalid("push carries no events"));
    }
    for event in incoming {
        if !event.at.is_finite() {
            return Err(invalid("event timestamp must be a finite number"));
        }
    }

    let mut replayed: Vec<HistoryEvent> = stored.iter().map(|entry| entry.event.clone()).collect();
    replayed.extend_from_slice(incoming);

    if stored.is_empty() {
        // Tier 1: nothing to diverge from, so the batch must stand on its
        // own as a whole history.
        return match ProjectHistory::from_events(replayed) {
            Ok(_) => Ok(PushValidation::Linear),
            Err(error) => Err(invalid(&format!("malformed history: {error}"))),
        };
    }

    if ProjectHistory::from_events(replayed).is_ok() {
        return Ok(PushValidation::Linear);
    }

    // Tier 2: the batch belongs to another line. Check what is checkable
    // without the pusher's base.
    check_internal_consistency(incoming)?;
    Ok(PushValidation::Divergent)
}

/// The checks that need no base: a project has exactly one origin event and
/// it is already stored, and a join always names two distinct versions.
fn check_internal_consistency(incoming: &[HistoryEvent]) -> Result<(), CloudError> {
    for event in incoming {
        if event.kind.is_origin() {
            return Err(invalid(
                "a project has exactly one origin event, and it is already recorded",
            ));
        }
        if let EventKind::Joined { kept, set_aside } = &event.kind
            && kept == set_aside
        {
            return Err(invalid("join must choose between two distinct versions"));
        }
    }
    Ok(())
}

fn invalid(detail: &str) -> CloudError {
    CloudError::InvalidRequest {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::ContentHash;

    fn saved(name: &[u8], at: f64) -> HistoryEvent {
        HistoryEvent {
            at,
            kind: EventKind::Saved {
                version: ContentHash::of(name),
            },
        }
    }

    fn created() -> HistoryEvent {
        HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        }
    }

    fn stored_log(events: &[HistoryEvent]) -> Vec<StoredEvent> {
        events
            .iter()
            .enumerate()
            .map(|(index, event)| StoredEvent {
                seq: index as u64 + 1,
                event: event.clone(),
            })
            .collect()
    }

    #[test]
    fn first_push_must_carry_an_origin() {
        assert!(matches!(
            validate_push_events(&[], &[saved(b"v1", 2.0)]),
            Err(CloudError::InvalidRequest { .. })
        ));
        assert_eq!(
            validate_push_events(&[], &[created(), saved(b"v1", 2.0)]),
            Ok(PushValidation::Linear)
        );
    }

    #[test]
    fn empty_batch_is_refused() {
        assert!(matches!(
            validate_push_events(&stored_log(&[created()]), &[]),
            Err(CloudError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn second_origin_is_refused() {
        let stored = stored_log(&[created(), saved(b"v1", 2.0)]);
        assert!(matches!(
            validate_push_events(&stored, &[created()]),
            Err(CloudError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn non_finite_timestamp_is_refused() {
        let stored = stored_log(&[created()]);
        assert!(matches!(
            validate_push_events(&stored, &[saved(b"v1", f64::NAN)]),
            Err(CloudError::InvalidRequest { .. })
        ));
    }

    /// The load-bearing case: a join authored against another line does not
    /// replay onto the interleaved server log, and is accepted anyway.
    #[test]
    fn join_from_another_line_is_divergent_not_rejected() {
        let stored = stored_log(&[created(), saved(b"v1", 2.0)]);
        let foreign_join = HistoryEvent {
            at: 3.0,
            kind: EventKind::Joined {
                kept: ContentHash::of(b"theirs"),
                set_aside: ContentHash::of(b"also-not-our-head"),
            },
        };
        assert_eq!(
            validate_push_events(&stored, &[foreign_join]),
            Ok(PushValidation::Divergent)
        );
    }

    #[test]
    fn degenerate_join_is_refused_even_when_divergent() {
        let stored = stored_log(&[created(), saved(b"v1", 2.0)]);
        let same = ContentHash::of(b"x");
        let degenerate = HistoryEvent {
            at: 3.0,
            kind: EventKind::Joined {
                kept: same,
                set_aside: same,
            },
        };
        assert!(matches!(
            validate_push_events(&stored, &[degenerate]),
            Err(CloudError::InvalidRequest { .. })
        ));
    }
}
