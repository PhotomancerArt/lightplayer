//! The fake board's end of its lp-link, as shipped firmware runs it (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`; USB by default, a classic's
//! UART link when the script's `link_config` says so): one board-side
//! [`Link`] per boot, a hello first on every `Up` (D5), replies as proto
//! messages (JSON, or learned packed payloads once the host opted in, D4),
//! and per-session state that starts over on every `Up` and `Reset`.

use lpc_wire::lp_link::{
    CH_PROTO, Link, LinkConfig, LinkEvent, Micros, SelectiveRepeat, SendError,
};
use lpc_wire::{
    ClientMessage, LearnStore, LearnedTable, LinkCounterTally, LinkCounters, WireEncoding,
    WireServerMessage, decode_client_payload, encode_server_payload,
};

/// What the host said on the link, for the device to act on.
#[derive(Debug)]
pub(crate) enum FakeLinkInput {
    /// A new session: the board says hello first.
    Up,
    /// One request.
    Request(ClientMessage),
    /// A proto message that is not a request (a host bug; logged).
    Malformed(String),
}

/// How one reply went.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FakeSendOutcome {
    /// Queued on the link; `packed` says in which form.
    Sent { packed: bool },
    /// The link's send budget is full: try again after the host acknowledges.
    Full,
    /// Bigger than one link message: dropped.
    TooBig,
}

/// One boot's link. See the module docs.
pub(crate) struct FakeBoardLink {
    link: Link<SelectiveRepeat>,
    up: bool,
    /// Whether this session's replies are packed (the host opted in).
    packed: bool,
    table: Box<LearnedTable>,
    /// Whether this session's hello has gone out.
    hello_sent: bool,
    tally: LinkCounterTally,
}

impl FakeBoardLink {
    /// One boot's link on `config` (the script's; USB unless a test asks
    /// for a classic-shaped UART link).
    pub(crate) fn new(config: LinkConfig, nonce: u32) -> Self {
        Self {
            link: Link::new(config, nonce),
            up: false,
            packed: false,
            table: LearnedTable::boxed(),
            hello_sent: false,
            tally: LinkCounterTally::new(),
        }
    }

    /// Bytes from the host.
    pub(crate) fn on_bytes(&mut self, now: Micros, bytes: &[u8]) {
        self.link.on_bytes(now, bytes);
    }

    /// Everything the host said since the last call, acting on the link's
    /// own events (a session's state starts over on `Up` and `Reset`).
    pub(crate) fn take_inputs(&mut self) -> Vec<FakeLinkInput> {
        let mut inputs = Vec::new();
        while let Some(event) = self.link.recv() {
            self.tally.note_event(&event);
            match event {
                LinkEvent::Up { .. } => {
                    self.start_session(true);
                    inputs.push(FakeLinkInput::Up);
                }
                LinkEvent::Reset { .. } => self.start_session(false),
                LinkEvent::Message {
                    channel: CH_PROTO,
                    data,
                } => inputs.push(match decode_client_payload(&data) {
                    Ok(request) => FakeLinkInput::Request(request),
                    Err(error) => FakeLinkInput::Malformed(error.to_string()),
                }),
                LinkEvent::Message { .. } | LinkEvent::Text(_) => {}
            }
        }
        inputs
    }

    /// Whether a session is up (replies can go out).
    pub(crate) fn is_up(&self) -> bool {
        self.up
    }

    /// Whether this session's hello has gone out.
    pub(crate) fn hello_sent(&self) -> bool {
        self.hello_sent
    }

    /// Queue one reply on the proto channel, packed when the host opted in.
    pub(crate) fn send(&mut self, message: &WireServerMessage) -> FakeSendOutcome {
        let mark = self.table.mark();
        let mut payload = Vec::new();
        let table: Option<&mut dyn LearnStore> = if self.packed {
            Some(&mut *self.table)
        } else {
            None
        };
        let packed = encode_server_payload(message, table, &mut payload);
        match self.link.send(CH_PROTO, &payload) {
            Ok(()) => {
                if matches!(message.msg, lpc_wire::ServerMsgBody::Hello(_)) {
                    self.hello_sent = true;
                }
                // The answer to an opt-in goes out in the old form; the
                // switch is for the replies after it.
                if let lpc_wire::ServerMsgBody::SetEncoding { encoding } = &message.msg {
                    self.packed = *encoding == WireEncoding::Packed;
                }
                FakeSendOutcome::Sent { packed }
            }
            Err(error) => {
                // Not sent: the table must not have learned from it.
                self.table.truncate(mark);
                match error {
                    SendError::Full => FakeSendOutcome::Full,
                    SendError::TooBig | SendError::BadChannel => FakeSendOutcome::TooBig,
                }
            }
        }
    }

    /// The frames the link wants on the wire now.
    pub(crate) fn transmit(&mut self, now: Micros, mut out: impl FnMut(&[u8])) {
        while let Some(frame) = self.link.poll_transmit(now) {
            out(frame);
        }
    }

    /// The first frame the link wants on the wire now, if any (the mid-frame
    /// cut takes exactly one).
    pub(crate) fn transmit_one(&mut self, now: Micros) -> Option<Vec<u8>> {
        self.link.poll_transmit(now).map(<[u8]>::to_vec)
    }

    /// This end's counters, for the heartbeat (D7).
    pub(crate) fn counters(&self) -> LinkCounters {
        self.tally.snapshot(self.link.counters())
    }

    fn start_session(&mut self, up: bool) {
        self.up = up;
        self.packed = false;
        self.table.reset(0);
        self.hello_sent = false;
    }
}
