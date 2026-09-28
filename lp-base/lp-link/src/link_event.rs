//! What the application receives from the link, in order.

use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkEvent {
    /// The link is up; `generation` numbers this session. Per-link state (the
    /// learned wire dictionary) starts fresh here.
    Up { generation: u32 },
    /// The session ended: every message sent before it that was not yet
    /// acknowledged may or may not have arrived. Per-link state must be
    /// dropped. `generation` is the new one.
    Reset {
        reason: ResetReason,
        generation: u32,
    },
    /// A whole message.
    Message { channel: u8, data: Vec<u8> },
    /// Bytes outside any frame: boot text, a panic, a raw print.
    Text(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetReason {
    /// The peer came back with a new nonce (reboot, page reload, replug).
    PeerRestarted,
    /// A frame went unacknowledged for `max_retries` sends.
    RetryLimit,
    /// A fragment that cannot belong to the message being reassembled.
    ProtocolError,
    /// The application asked (`Link::restart`).
    Requested,
}
