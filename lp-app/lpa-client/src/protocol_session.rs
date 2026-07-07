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
    /// Ids of requests this client stopped waiting for (cancelled or
    /// timed-out pulls). The server does not know the client walked away, so
    /// it may still deliver frames for these ids; those late arrivals are
    /// correct-by-design discards and classify as
    /// [`ResponseDisposition::StaleAbandoned`], not `Uncorrelated`.
    abandoned_request_ids: Vec<u64>,
}

impl ProtocolSession {
    pub fn new() -> Self {
        Self {
            next_request_id: 1,
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
