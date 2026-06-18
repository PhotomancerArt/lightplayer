//! Request id allocation and response classification for `lp-server`.
//!
//! Keeping this separate lets host and browser adapters share correlation
//! behavior even when their I/O mechanics differ.

use lpc_wire::WireServerMessage;

/// Per-connection protocol state.
#[derive(Debug, Clone)]
pub struct ProtocolSession {
    next_request_id: u64,
}

impl ProtocolSession {
    pub fn new() -> Self {
        Self { next_request_id: 1 }
    }

    pub fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
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
    /// A response for a different request arrived.
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

    fn message(id: u64) -> WireServerMessage {
        WireServerMessage {
            id,
            msg: ServerMsgBody::StopAllProjects,
        }
    }
}
