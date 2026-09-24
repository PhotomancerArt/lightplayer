//! Hosts: when to ask a board to pack its replies, and when to ask again.
//!
//! The opt-in ([`ClientRequest::SetEncoding`]) is per link, and a board
//! drops it without a word: on a USB de-enumerate, on a reboot, and when the
//! host stops draining for a while (that is how a board on USB-Serial-JTAG
//! detects a closed port, so a merely slow host trips it too). The reader
//! never breaks — every host reader accepts both forms
//! ([`WireStream`](crate::wire_stream::WireStream)) — but the link has quietly
//! gone back to JSON, and nothing else would notice.
//!
//! [`PackOptIn`] notices. It watches every message the board sends, with the
//! form it came in, and says when to write the opt-in:
//!
//! - **first**, as soon as a [`ServerHello`](crate::ServerHello) (the boot
//!   hello or the answer to a `Hello` request) says the board packs with this
//!   build's dictionary — `pack_dictionary == WIRE_DICTIONARY_FINGERPRINT` and
//!   the same `proto`. A board that cannot pack (`0`) or packs with another
//!   dictionary is never asked;
//! - **again**, when a JSON message arrives on a link that was agreed packed:
//!   the board fell back. Asks are rate-limited to one per
//!   [`PACK_REASK_INTERVAL_MS`], so a board that keeps falling back costs one
//!   request every few seconds and never a loop. The same limit re-asks an
//!   opt-in whose answer never came. A board also sends a single reply as
//!   JSON when it does not fit its frame buffer packed; that reads as a
//!   fallback too and costs one redundant opt-in (answered `packed`), which
//!   is cheaper than telling the two apart.
//!
//! The opt-in goes out with its own id, [`PACK_OPT_IN_REQUEST_ID`], and its
//! answer is this module's, not the caller's: [`PackOptInStep::deliver`] is
//! `false` for it, so the transport's consumer never sees a reply to a
//! request it did not make. A board that answers `json` is not asked again
//! until [`PackOptIn::reset`].
//!
//! Sans-IO: time is the caller's (`now_ms`, any monotonic millisecond count),
//! and the request is handed back to write, never written.

use crate::message::client::{ClientMessage, ClientRequest};
use crate::server::ServerMsgBody;
use crate::{WIRE_DICTIONARY_FINGERPRINT, WIRE_PROTO_VERSION, WireEncoding, WireServerMessage};

/// The id the opt-in is sent with: `2^53 - 1`, far above any request
/// counter and still exact as a JavaScript number.
pub const PACK_OPT_IN_REQUEST_ID: u64 = (1 << 53) - 1;

/// The least time between two opt-ins on one link.
pub const PACK_REASK_INTERVAL_MS: u64 = 3_000;

/// Where one link stands with the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agreement {
    /// JSON, and not asked (or the ask is due again).
    Json,
    /// Asked; the answer has not come.
    Asked,
    /// The board said `packed`, or sent a packed frame.
    Packed,
    /// The board answered `json`. Not asked again on this link.
    Refused,
}

/// What to do with one message the board sent.
#[derive(Debug, Clone)]
pub struct PackOptInStep {
    /// Hand the message on. `false` only for the answer to this module's own
    /// opt-in.
    pub deliver: bool,
    /// Write this request to the board now (as JSON, like every request).
    pub send: Option<ClientMessage>,
}

/// One link's opt-in state. See the module docs.
#[derive(Debug, Clone)]
pub struct PackOptIn {
    wanted: bool,
    board_can_pack: bool,
    agreement: Agreement,
    last_ask_ms: Option<u64>,
}

impl PackOptIn {
    /// A link that will ask for packed replies when `wanted` (a host that
    /// wants JSON passes `false` and this never asks).
    pub const fn new(wanted: bool) -> Self {
        Self {
            wanted,
            board_can_pack: false,
            agreement: Agreement::Json,
            last_ask_ms: None,
        }
    }

    /// A link that asks when `encoding` is [`WireEncoding::Packed`].
    pub const fn wanting(encoding: WireEncoding) -> Self {
        Self::new(matches!(encoding, WireEncoding::Packed))
    }

    /// The encoding the board is writing this link's replies in, as far as
    /// this side knows.
    pub fn encoding(&self) -> WireEncoding {
        match self.agreement {
            Agreement::Packed => WireEncoding::Packed,
            _ => WireEncoding::Json,
        }
    }

    /// Observe one message the board sent, `packed` when it came as a packed
    /// frame, at `now_ms`.
    pub fn observe(
        &mut self,
        message: &WireServerMessage,
        packed: bool,
        now_ms: u64,
    ) -> PackOptInStep {
        // The form first: the answer to the opt-in is always JSON, and must
        // not read as a fallback from the agreement it makes.
        if packed {
            self.agreement = Agreement::Packed;
        } else if self.agreement == Agreement::Packed {
            self.agreement = Agreement::Json;
        }

        let mut deliver = true;
        match &message.msg {
            ServerMsgBody::Hello(hello) => {
                self.board_can_pack = hello.proto == WIRE_PROTO_VERSION
                    && hello.pack_dictionary == WIRE_DICTIONARY_FINGERPRINT;
            }
            ServerMsgBody::SetEncoding { encoding } if message.id == PACK_OPT_IN_REQUEST_ID => {
                deliver = false;
                self.agreement = match encoding {
                    WireEncoding::Packed => Agreement::Packed,
                    WireEncoding::Json => Agreement::Refused,
                };
            }
            _ => {}
        }

        PackOptInStep {
            deliver,
            send: self.ask_if_due(now_ms),
        }
    }

    /// The link closed or reopened: the board has forgotten the opt-in, and
    /// what it can do is learned again from its next hello.
    pub fn reset(&mut self) {
        *self = Self::new(self.wanted);
    }

    fn ask_if_due(&mut self, now_ms: u64) -> Option<ClientMessage> {
        let waiting = matches!(self.agreement, Agreement::Json | Agreement::Asked);
        let due = self
            .last_ask_ms
            .is_none_or(|at| now_ms.saturating_sub(at) >= PACK_REASK_INTERVAL_MS);
        if !(self.wanted && self.board_can_pack && waiting && due) {
            return None;
        }
        self.agreement = Agreement::Asked;
        self.last_ask_ms = Some(now_ms);
        Some(ClientMessage {
            id: PACK_OPT_IN_REQUEST_ID,
            msg: ClientRequest::SetEncoding {
                encoding: WireEncoding::Packed,
                dictionary: WIRE_DICTIONARY_FINGERPRINT,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::hello::{BuildFacts, HardwareFacts, ServerHello};
    use alloc::string::ToString;
    use alloc::vec::Vec;

    #[test]
    fn a_board_that_packs_with_our_dictionary_is_asked_at_its_hello() {
        let mut link = PackOptIn::new(true);

        let step = link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, 0);

        assert!(step.deliver, "the hello is the caller's");
        let ask = step.send.expect("asked at the hello");
        assert_eq!(ask.id, PACK_OPT_IN_REQUEST_ID);
        assert!(matches!(
            ask.msg,
            ClientRequest::SetEncoding {
                encoding: WireEncoding::Packed,
                dictionary: WIRE_DICTIONARY_FINGERPRINT,
            }
        ));
        assert_eq!(link.encoding(), WireEncoding::Json);

        let answer = link.observe(&answer(WireEncoding::Packed), false, 5);
        assert!(!answer.deliver, "the answer is the opt-in's own");
        assert!(answer.send.is_none());
        assert_eq!(link.encoding(), WireEncoding::Packed);

        let reply = link.observe(&other(3), true, 10);
        assert!(reply.deliver && reply.send.is_none());
        assert_eq!(link.encoding(), WireEncoding::Packed);
    }

    #[test]
    fn a_board_that_cannot_pack_or_packs_otherwise_is_never_asked() {
        for fingerprint in [0, WIRE_DICTIONARY_FINGERPRINT ^ 1] {
            let mut link = PackOptIn::new(true);
            assert!(link.observe(&hello(fingerprint), false, 0).send.is_none());
            assert!(link.observe(&other(0), false, 60_000).send.is_none());
        }
        // Nor a board on another proto.
        let mut link = PackOptIn::new(true);
        let mut hello = hello(WIRE_DICTIONARY_FINGERPRINT);
        if let ServerMsgBody::Hello(h) = &mut hello.msg {
            h.proto += 1;
        }
        assert!(link.observe(&hello, false, 0).send.is_none());
    }

    #[test]
    fn a_host_that_wants_json_never_asks() {
        let mut link = PackOptIn::wanting(WireEncoding::Json);
        assert!(
            link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, 0)
                .send
                .is_none()
        );
    }

    /// A JSON message on a link agreed packed is the board falling back
    /// (a slow host tripped its closed-port detection): ask again — once per
    /// interval, never in a loop.
    #[test]
    fn a_fallback_to_json_is_asked_again_at_most_once_per_interval() {
        let mut link = agreed_packed_at(0);

        // In the same interval as the first ask: noticed, not asked yet.
        let early = link.observe(&other(0), false, 1_000);
        assert!(early.deliver);
        assert!(early.send.is_none(), "rate-limited");
        assert_eq!(link.encoding(), WireEncoding::Json);

        // Heartbeats keep coming as JSON, one a second; every one past the
        // interval since the last ask re-asks, and none in between does.
        let asked_at: Vec<u64> = (2..=21)
            .map(|second| second * 1_000)
            .filter(|&t| link.observe(&other(0), false, t).send.is_some())
            .collect();
        assert_eq!(
            asked_at,
            [3_000, 6_000, 9_000, 12_000, 15_000, 18_000, 21_000]
        );

        // The board takes it back up.
        link.observe(&answer(WireEncoding::Packed), false, 30_000);
        assert_eq!(link.encoding(), WireEncoding::Packed);
    }

    #[test]
    fn a_refusal_is_not_asked_again_until_the_link_resets() {
        let mut link = PackOptIn::new(true);
        link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, 0);
        let refused = link.observe(&answer(WireEncoding::Json), false, 1);
        assert!(!refused.deliver);
        for t in [10_000, 20_000, 30_000] {
            assert!(link.observe(&other(0), false, t).send.is_none());
        }

        link.reset();
        assert!(
            link.observe(&other(0), false, 40_000).send.is_none(),
            "a reset link waits for the next hello"
        );
        assert!(
            link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, 40_001)
                .send
                .is_some()
        );
    }

    /// A reboot: the boot hello arrives as JSON on a link agreed packed.
    #[test]
    fn a_reboot_is_a_fallback_and_its_hello_is_delivered() {
        let mut link = agreed_packed_at(0);
        let step = link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, 10_000);
        assert!(step.deliver);
        assert!(step.send.is_some());
    }

    /// Only the opt-in's own answer is swallowed.
    #[test]
    fn another_set_encoding_answer_is_delivered() {
        let mut link = PackOptIn::new(true);
        let mut msg = answer(WireEncoding::Packed);
        msg.id = 12;
        assert!(link.observe(&msg, false, 0).deliver);
    }

    fn agreed_packed_at(now_ms: u64) -> PackOptIn {
        let mut link = PackOptIn::new(true);
        assert!(
            link.observe(&hello(WIRE_DICTIONARY_FINGERPRINT), false, now_ms)
                .send
                .is_some()
        );
        link.observe(&answer(WireEncoding::Packed), false, now_ms);
        assert_eq!(link.encoding(), WireEncoding::Packed);
        link
    }

    fn hello(pack_dictionary: u32) -> WireServerMessage {
        let hello = ServerHello {
            proto: WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: alloc::vec![],
                package: "fw-esp32c6".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts::default(),
            device_uid: None,
            pack_dictionary,
        };
        WireServerMessage::new(0, ServerMsgBody::Hello(hello))
    }

    fn answer(encoding: WireEncoding) -> WireServerMessage {
        WireServerMessage::new(
            PACK_OPT_IN_REQUEST_ID,
            ServerMsgBody::SetEncoding { encoding },
        )
    }

    fn other(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, ServerMsgBody::StopAllProjects)
    }
}
