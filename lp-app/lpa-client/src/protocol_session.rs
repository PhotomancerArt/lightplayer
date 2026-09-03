//! Request id allocation and response classification for `lp-server`.
//!
//! Keeping this separate lets host and browser adapters share correlation
//! behavior even when their I/O mechanics differ.

use lpc_wire::WireServerMessage;

/// How many abandoned request ids the session remembers for stale-response
/// classification. Late frames of an abandoned request arrive during the
/// request(s) immediately following it (the transport is ordered), so only
/// the most recent abandonments matter; the bound keeps the session O(1)
/// through arbitrarily long cancel-heavy sessions (e.g. drag floods).
const MAX_ABANDONED_REQUEST_IDS: usize = 32;

/// Per-connection protocol state.
#[derive(Debug, Clone)]
pub struct ProtocolSession {
    next_request_id: u64,
    /// The first id this session ever allocates. Responses carrying a
    /// smaller (non-zero) id answer requests issued by a PREVIOUS owner of
    /// the same wire — a lens client that took over a roster device's port
    /// mid-conversation — and are dropped quietly as that owner's, never
    /// mistaken for this session's own reply ([`ResponseDisposition::PriorOwner`]).
    first_request_id: u64,
    /// Ids of requests this client stopped waiting for (cancelled or
    /// timed-out pulls). The server does not know the client walked away, so
    /// it may still deliver frames for these ids; those late arrivals are
    /// correct-by-design discards and classify as
    /// [`ResponseDisposition::StaleAbandoned`], not `Uncorrelated`.
    abandoned_request_ids: Vec<u64>,
}

impl ProtocolSession {
    pub fn new() -> Self {
        Self::starting_at(1)
    }

    /// A session whose request ids start at `first_request_id` instead of 1,
    /// so a client taking over a wire another id space was using (the
    /// editor lens on a roster device's port) cannot collide with that
    /// space's in-flight replies. `0` is the unsolicited id and is never
    /// allocated; a start of `0` behaves like `1`.
    pub fn starting_at(first_request_id: u64) -> Self {
        let first_request_id = first_request_id.max(1);
        Self {
            next_request_id: first_request_id,
            first_request_id,
            abandoned_request_ids: Vec::new(),
        }
    }

    pub fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    /// Record a request id whose response(s) this client will no longer
    /// consume (the pull loop was cancelled or its progress deadline fired).
    /// Late frames carrying this id are then expected and classified as
    /// [`ResponseDisposition::StaleAbandoned`]. Bounded FIFO: only the most
    /// recent [`MAX_ABANDONED_REQUEST_IDS`] abandonments are remembered.
    pub fn abandon_request(&mut self, request_id: u64) {
        if self.abandoned_request_ids.contains(&request_id) {
            return;
        }
        if self.abandoned_request_ids.len() == MAX_ABANDONED_REQUEST_IDS {
            self.abandoned_request_ids.remove(0);
        }
        self.abandoned_request_ids.push(request_id);
    }

    pub fn response_disposition(
        &self,
        response: &WireServerMessage,
        expected_id: u64,
    ) -> ResponseDisposition {
        if response.id == expected_id {
            ResponseDisposition::Matched
        } else if response.id == 0 {
            ResponseDisposition::Unsolicited
        } else if self.abandoned_request_ids.contains(&response.id) {
            ResponseDisposition::StaleAbandoned {
                response_id: response.id,
            }
        } else if response.id < self.first_request_id {
            ResponseDisposition::PriorOwner {
                response_id: response.id,
            }
        } else {
            ResponseDisposition::Uncorrelated {
                response_id: response.id,
                expected_id,
            }
        }
    }
}

impl Default for ProtocolSession {
    fn default() -> Self {
        Self::new()
    }
}

/// How an incoming server message relates to the request currently in flight.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ResponseDisposition {
    /// The response id matches the request id we are waiting for.
    Matched,
    /// Server-originated event such as heartbeat/log.
    Unsolicited,
    /// A late response for a request this client abandoned (cancelled or
    /// timed-out pull). Dropping it is the designed behaviour, so callers
    /// should discard quietly (at most a debug-level note), not warn.
    StaleAbandoned { response_id: u64 },
    /// A response to a request this session never issued because it
    /// predates the session's id space: the wire's previous owner asked it
    /// (see [`ProtocolSession::starting_at`]). Expected on a handed-over
    /// wire; dropping it is correct, and the previous owner still hears it
    /// through whatever tap it kept. Callers should discard quietly.
    PriorOwner { response_id: u64 },
    /// A response id this session never abandoned and is not waiting for:
    /// an id from the future, or a duplicate delivery of an already-consumed
    /// response. Genuinely unexpected — callers should warn.
    Uncorrelated { response_id: u64, expected_id: u64 },
}

#[cfg(test)]
mod tests {
    use lpc_wire::WireServerMessage;
    use lpc_wire::server::ServerMsgBody;

    use super::*;

    #[test]
    fn request_ids_start_at_one_and_increment() {
        let mut session = ProtocolSession::new();

        assert_eq!(session.next_request_id(), 1);
        assert_eq!(session.next_request_id(), 2);
    }

    #[test]
    fn classifies_response_ids() {
        let session = ProtocolSession::new();

        assert_eq!(
            session.response_disposition(&message(7), 7),
            ResponseDisposition::Matched
        );
        assert_eq!(
            session.response_disposition(&message(0), 7),
            ResponseDisposition::Unsolicited
        );
        assert_eq!(
            session.response_disposition(&message(9), 7),
            ResponseDisposition::Uncorrelated {
                response_id: 9,
                expected_id: 7
            }
        );
    }

    #[test]
    fn ids_below_the_session_start_belong_to_the_prior_owner() {
        let mut session = ProtocolSession::starting_at(1 << 32);
        let mine = session.next_request_id();
        assert_eq!(mine, 1 << 32);

        // The previous owner's in-flight reply (a roster activity's small
        // counter) is theirs, not an uncorrelated surprise…
        assert_eq!(
            session.response_disposition(&message(1), mine),
            ResponseDisposition::PriorOwner { response_id: 1 }
        );
        // …the unsolicited id stays unsolicited…
        assert_eq!(
            session.response_disposition(&message(0), mine),
            ResponseDisposition::Unsolicited
        );
        // …and an id from this session's own future is still uncorrelated.
        assert_eq!(
            session.response_disposition(&message(mine + 1), mine),
            ResponseDisposition::Uncorrelated {
                response_id: mine + 1,
                expected_id: mine
            }
        );
        // A start of 0 never allocates the unsolicited id.
        assert_eq!(ProtocolSession::starting_at(0).next_request_id(), 1);
    }

    #[test]
    fn abandoned_request_ids_classify_as_stale_not_uncorrelated() {
        let mut session = ProtocolSession::new();
        let abandoned = session.next_request_id();
        let expected = session.next_request_id();
        session.abandon_request(abandoned);

        // The late response for the abandoned id is an expected discard.
        assert_eq!(
            session.response_disposition(&message(abandoned), expected),
            ResponseDisposition::StaleAbandoned {
                response_id: abandoned
            }
        );
        // An id the session never issued nor abandoned still warns.
        assert_eq!(
            session.response_disposition(&message(99), expected),
            ResponseDisposition::Uncorrelated {
                response_id: 99,
                expected_id: expected
            }
        );
    }

    #[test]
    fn abandoned_id_memory_is_bounded_to_the_most_recent() {
        let mut session = ProtocolSession::new();
        for id in 1..=40 {
            session.abandon_request(id);
        }

        // The oldest abandonment was evicted; the most recent ones remain.
        assert!(matches!(
            session.response_disposition(&message(1), 41),
            ResponseDisposition::Uncorrelated { .. }
        ));
        assert!(matches!(
            session.response_disposition(&message(40), 41),
            ResponseDisposition::StaleAbandoned { response_id: 40 }
        ));
    }

    fn message(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, ServerMsgBody::StopAllProjects)
    }
}
