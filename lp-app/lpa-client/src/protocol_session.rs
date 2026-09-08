//! Request id allocation and response classification for `lp-server`.
//!
//! Keeping this separate lets host and browser adapters share correlation
//! behavior even when their I/O mechanics differ.

use core::sync::atomic::{AtomicU64, Ordering};

use lpc_wire::{ClientRequest, WireServerMessage, WireServerMsgBody};

/// Where the id space of a conversation on a BORROWED wire starts.
///
/// A coarse effect (push, remove, manifest stamp) and the editor lens both
/// take a device's wire away from the model's pump for the length of one
/// conversation. The model was already talking on that wire with ids of
/// its own — the identify hello re-ask counts from 1 — and the board's
/// answer to its last ask can still be in the buffer when the borrow
/// starts. A conversation that also counted from 1 would read that answer,
/// find the id it is waiting for, and hand a `Hello` back as the reply to
/// `project.list_loaded` (the flake this constant exists to make
/// impossible: CI, 2026-09-07).
///
/// The model and its coarse effects number their frames from 1 in small
/// `u32` counters, so starting far above the whole `u32` range makes every
/// straggler of theirs classify as [`ResponseDisposition::PriorOwner`] — a
/// quiet discard, and for the lens still the roster's own evidence through
/// the lens tap — whatever the timing was.
pub const BORROWED_WIRE_REQUEST_ID_BASE: u64 = 1 << 32;

/// How far apart two borrowed-wire conversations' id spaces sit.
///
/// The base alone only separates a conversation from the MODEL. Two
/// conversations in a row on the same wire (a push, then a removal) would
/// share one id space, and a straggler from the first could land on the id
/// the second is waiting for — the same collision one level up. So each
/// conversation takes the next stride, and no conversation ever mints an
/// id an earlier one could have used. 16.7 M requests per conversation is
/// far past any conversation this protocol has (a chunked write of a large
/// project is thousands), and a `u64` holds ~2^40 conversations.
const BORROWED_WIRE_ID_STRIDE: u64 = 1 << 24;

/// The next unused borrowed-wire id space. Process-global because the
/// wires are: two studios in one process would still each need their own.
static NEXT_BORROWED_WIRE_BASE: AtomicU64 = AtomicU64::new(BORROWED_WIRE_REQUEST_ID_BASE);

/// Claim the next borrowed-wire id space, for a conversation taking a
/// device's wire over from the model's pump (or from an earlier
/// conversation). See [`BORROWED_WIRE_REQUEST_ID_BASE`].
pub fn next_borrowed_wire_request_id_base() -> u64 {
    NEXT_BORROWED_WIRE_BASE.fetch_add(BORROWED_WIRE_ID_STRIDE, Ordering::Relaxed)
}

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

    /// Where an arriving frame belongs, given the request in flight.
    ///
    /// `asked` is what the pending request asked for, and it is not
    /// decoration: an id match alone is NOT proof that a frame answers the
    /// request. A request id is only unique within one client's id space,
    /// and a device's wire carries several — the device model's own small
    /// counters, an effect that borrowed the port, the editor lens — so a
    /// straggler from another space can arrive bearing the very id this
    /// session is waiting on. Frames the server sends on its OWN initiative
    /// (a hello, a heartbeat, a log) are the ones that make the collision
    /// dangerous, because they can appear at ANY time, so they are ruled out
    /// as answers before the id is consulted for a MATCH
    /// ([`ResponseDisposition::ServerOriginated`]). The single exception is
    /// a hello the client ASKED for: `Hello` is both the bootstrap
    /// announcement and the response to [`ClientRequest::Hello`].
    ///
    /// A response-shaped id is still consulted first for one thing: whether
    /// it predates this session's id space entirely
    /// ([`ResponseDisposition::PriorOwner`]). A wire handed over from a
    /// previous owner (the roster model's pump, or an earlier borrow) can
    /// still be carrying that owner's OWN server-originated straggler — its
    /// unanswered identify hello, say — and that frame is the previous
    /// owner's business, not this session's event stream, whether or not it
    /// happens to look like an answer.
    pub fn response_disposition(
        &self,
        response: &WireServerMessage,
        expected_id: u64,
        asked: PendingAsk,
    ) -> ResponseDisposition {
        let answers_anything = asked == PendingAsk::Hello || !server_originated(&response.msg);
        if response.id == expected_id && answers_anything {
            ResponseDisposition::Matched
        } else if response.id == 0 {
            ResponseDisposition::Unsolicited
        } else if response.id < self.first_request_id {
            ResponseDisposition::PriorOwner {
                response_id: response.id,
            }
        } else if !answers_anything {
            ResponseDisposition::ServerOriginated {
                response_id: response.id,
            }
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

/// Whether the server sends this body on its own initiative rather than as
/// an answer to something.
///
/// These three are exactly the bodies [`crate::ClientEvent`] can carry, and
/// that is not a coincidence: a frame that is a side-channel event is, by
/// the same token, never a reply. `Hello` is the one that also serves as a
/// response — but only to [`ClientRequest::Hello`], which the caller states
/// via [`PendingAsk`].
fn server_originated(body: &WireServerMsgBody) -> bool {
    matches!(
        body,
        WireServerMsgBody::Hello(_)
            | WireServerMsgBody::Heartbeat { .. }
            | WireServerMsgBody::Log { .. }
    )
}

/// What the request in flight asked for — the half of correlation the id
/// cannot supply.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PendingAsk {
    /// [`ClientRequest::Hello`]: this request, and only this request, is
    /// answered by a `Hello` body.
    Hello,
    /// Anything else. A `Hello` arriving under this request's id is another
    /// id space's straggler, never this request's reply.
    Other,
}

impl PendingAsk {
    /// What `request` asks for.
    pub fn of(request: &ClientRequest) -> Self {
        match request {
            ClientRequest::Hello => Self::Hello,
            _ => Self::Other,
        }
    }
}

/// How an incoming server message relates to the request currently in flight.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ResponseDisposition {
    /// The response id matches the request id we are waiting for.
    Matched,
    /// Server-originated event such as heartbeat/log, on the unsolicited
    /// id (0) the protocol reserves for them.
    Unsolicited,
    /// A frame the server originated — a hello, a heartbeat, a log —
    /// carrying a REQUEST id rather than the unsolicited 0.
    ///
    /// On a wire whose id space has one owner this cannot happen. On a
    /// device's wire it can and does: the device model asks for a hello
    /// with its own counter, an effect borrows the port and starts its own
    /// counter at 1, and the model's answer lands in the effect's stream
    /// bearing an id the effect is waiting on. Correlating on the id alone
    /// would hand a `Hello` to whatever asked — which is how a push once
    /// reported `unexpected response for project.list_loaded: Hello(…)`.
    ///
    /// Callers treat it like [`Self::Unsolicited`]: surface it as an event
    /// (the identity in it is still true) and keep waiting for the answer.
    ServerOriginated { response_id: u64 },
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
            session.response_disposition(&message(7), 7, PendingAsk::Other),
            ResponseDisposition::Matched
        );
        assert_eq!(
            session.response_disposition(&message(0), 7, PendingAsk::Other),
            ResponseDisposition::Unsolicited
        );
        assert_eq!(
            session.response_disposition(&message(9), 7, PendingAsk::Other),
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
            session.response_disposition(&message(1), mine, PendingAsk::Other),
            ResponseDisposition::PriorOwner { response_id: 1 }
        );
        // …the unsolicited id stays unsolicited…
        assert_eq!(
            session.response_disposition(&message(0), mine, PendingAsk::Other),
            ResponseDisposition::Unsolicited
        );
        // …and an id from this session's own future is still uncorrelated.
        assert_eq!(
            session.response_disposition(&message(mine + 1), mine, PendingAsk::Other),
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
            session.response_disposition(&message(abandoned), expected, PendingAsk::Other),
            ResponseDisposition::StaleAbandoned {
                response_id: abandoned
            }
        );
        // An id the session never issued nor abandoned still warns.
        assert_eq!(
            session.response_disposition(&message(99), expected, PendingAsk::Other),
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
            session.response_disposition(&message(1), 41, PendingAsk::Other),
            ResponseDisposition::Uncorrelated { .. }
        ));
        assert!(matches!(
            session.response_disposition(&message(40), 41, PendingAsk::Other),
            ResponseDisposition::StaleAbandoned { response_id: 40 }
        ));
    }

    /// The defect this rule exists for: a hello bearing the very id a
    /// non-hello request is waiting on. The device model asks for a hello
    /// with its own counter starting at 1; a borrowed-port conversation
    /// starts its counter at 1 too, and the model's answer can still be on
    /// the wire when the borrow's first request goes out.
    #[test]
    fn a_hello_never_answers_a_request_that_did_not_ask_for_one() {
        let session = ProtocolSession::new();

        assert_eq!(
            session.response_disposition(&hello(1), 1, PendingAsk::Other),
            ResponseDisposition::ServerOriginated { response_id: 1 }
        );
        // …but the hello a client DID ask for is still its answer.
        assert_eq!(
            session.response_disposition(&hello(1), 1, PendingAsk::Hello),
            ResponseDisposition::Matched
        );
        // The unsolicited id stays unsolicited whatever was asked.
        assert_eq!(
            session.response_disposition(&hello(0), 1, PendingAsk::Other),
            ResponseDisposition::Unsolicited
        );
        // A hello on some OTHER live id is the same straggler, named as
        // such rather than as an uncorrelated surprise.
        assert_eq!(
            session.response_disposition(&hello(9), 1, PendingAsk::Other),
            ResponseDisposition::ServerOriginated { response_id: 9 }
        );
    }

    /// `PendingAsk::of` is what carries the exception, so it must name
    /// exactly one request.
    #[test]
    fn only_a_hello_request_expects_a_hello() {
        assert_eq!(
            PendingAsk::of(&lpc_wire::ClientRequest::Hello),
            PendingAsk::Hello
        );
        assert_eq!(
            PendingAsk::of(&lpc_wire::ClientRequest::ListLoadedProjects),
            PendingAsk::Other
        );
    }

    fn message(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, ServerMsgBody::StopAllProjects)
    }

    fn hello(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            ServerMsgBody::Hello(lpc_wire::server::hello::ServerHello {
                proto: lpc_wire::WIRE_PROTO_VERSION,
                build: lpc_wire::server::hello::BuildFacts {
                    features: Vec::new(),
                    package: "fw-esp32c6".to_string(),
                    commit: "fake-firmware".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: Default::default(),
                device_uid: Some("dev000000daqf6dvvt2".to_string()),
            }),
        )
    }
}
