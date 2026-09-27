//! The device link's counters, as the heartbeat carries them.
//!
//! Every recovery lp-link makes (a resend, a damaged frame dropped, a frame
//! from an older session, a reset) is counted, and the board reports its end
//! of the link on every [`ServerMsgBody::Heartbeat`](crate::ServerMsgBody::Heartbeat),
//! so a lossy edge (a host tty, an ISR, a cable) stays visible instead of
//! being silently papered over: lp-link's principle 2, "count every recovery,
//! never hide it" (`lp-base/lp-link/README.md`). A host keeps the same
//! record of its own end ([`WireLinkPort::counters`](crate::WireLinkPort::counters)).
//!
//! This replaces the `M!`-line counters (parse failures, the not-draining
//! latch's stamps) at `WIRE_PROTO_VERSION` 30, when the USB link moved onto
//! lp-link (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D7).
//!
//! Most fields come straight from [`lp_link::LinkCounters`]; resets by
//! reason, stalls and payload errors are the edge's to count (lp-link hands
//! them up as events and states), which [`LinkCounterTally`](crate::LinkCounterTally)
//! does for both ends.

use serde::{Deserialize, Serialize};

/// One end of a device link's counters, monotonic since that end started
/// (a board: since boot; a host: since the port opened). See the module docs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LinkCounters {
    /// Frames written (every kind: data, acknowledgements, handshakes).
    pub frames_tx: u32,
    /// Frames that arrived and verified.
    pub frames_rx: u32,
    /// Bytes written to the transport.
    pub bytes_tx: u64,
    /// Bytes read from the transport (console text included).
    pub bytes_rx: u64,
    /// Reliable frames sent again: what the link resent because the first
    /// copy was lost, damaged, or late. About zero on a clean cable.
    pub resends: u32,
    /// Frames that failed their checksum, their COBS, their header, or were
    /// too long: damaged on the way, and dropped (the sender resends them).
    pub damaged: u32,
    /// Frames that verified only under the previous session's key: in flight
    /// across a reset, and dropped.
    pub stale_session: u32,
    /// Partial frames abandoned: gone quiet mid-frame, or cut short by a
    /// raw text mark (a panic).
    pub stale_partials: u32,
    /// Frames received twice (a resend whose first copy did arrive).
    pub duplicates: u32,
    /// Frames refused because the application was not reading fast enough.
    pub rx_no_room: u32,
    /// Best-effort messages (log records) the peer sent that never arrived.
    pub datagrams_lost: u32,
    /// Best-effort messages this end dropped before sending (queue full).
    pub datagrams_dropped: u32,
    /// Times the link came up.
    pub ups: u32,
    /// Times the session ended, and why.
    pub resets: LinkResets,
    /// Times the peer went quiet past the link's stall time while the link
    /// was up (a cable out, a hung page); the session was kept.
    pub stalls: u32,
    /// Proto-channel messages that arrived intact and did not decode: a bug
    /// over a reliable link (the receiving end restarts the link on each).
    pub payload_errors: u32,
}

/// Resets, in total and by reason.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LinkResets {
    /// Every reset, whatever its reason (lp-link's own count).
    pub total: u32,
    /// The peer came back with a new session: a reboot, a reload, a replug.
    pub peer_restarted: u32,
    /// A frame went unacknowledged for the link's retry limit.
    pub retry_limit: u32,
    /// A frame that cannot belong to the message being reassembled.
    pub protocol_error: u32,
    /// This end asked (`Link::restart`), e.g. after a payload error.
    pub requested: u32,
}

impl From<&lp_link::LinkCounters> for LinkCounters {
    /// What lp-link counts itself. Resets by reason (other than protocol
    /// errors), stalls and payload errors stay zero: they are the edge's to
    /// count, with [`LinkCounterTally`](crate::LinkCounterTally).
    fn from(c: &lp_link::LinkCounters) -> Self {
        LinkCounters {
            frames_tx: c.frames_tx,
            frames_rx: c.frames_rx,
            bytes_tx: c.bytes_tx,
            bytes_rx: c.bytes_rx,
            resends: c.retransmits,
            damaged: c.bad_frames.saturating_add(c.oversize_frames),
            stale_session: c.stale_frames,
            stale_partials: c.stale_partials,
            duplicates: c.duplicates,
            rx_no_room: c.rx_no_room,
            datagrams_lost: c.datagrams_lost,
            datagrams_dropped: c.datagrams_dropped,
            ups: c.ups,
            resets: LinkResets {
                total: c.resets,
                protocol_error: c.protocol_errors,
                ..LinkResets::default()
            },
            stalls: 0,
            payload_errors: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lp_link_counts_map_onto_the_wire_names() {
        let link = lp_link::LinkCounters {
            frames_tx: 10,
            frames_rx: 9,
            bytes_tx: 1_000,
            bytes_rx: 900,
            retransmits: 3,
            bad_frames: 2,
            oversize_frames: 1,
            stale_frames: 4,
            stale_partials: 5,
            duplicates: 6,
            rx_no_room: 7,
            datagrams_lost: 8,
            datagrams_dropped: 11,
            ups: 2,
            resets: 1,
            protocol_errors: 1,
            ..lp_link::LinkCounters::default()
        };
        let wire = LinkCounters::from(&link);
        assert_eq!(wire.resends, 3);
        assert_eq!(wire.damaged, 3, "bad and oversize frames are both damage");
        assert_eq!(wire.stale_session, 4);
        assert_eq!(wire.resets.total, 1);
        assert_eq!(wire.resets.protocol_error, 1);
        assert_eq!(wire.resets.peer_restarted, 0, "the edge's to count");
    }

    #[test]
    fn the_wire_spelling_is_camel_case_and_whole() {
        let json = crate::json::to_string(&LinkCounters::default()).unwrap();
        assert_eq!(
            json,
            r#"{"framesTx":0,"framesRx":0,"bytesTx":0,"bytesRx":0,"resends":0,"damaged":0,"staleSession":0,"stalePartials":0,"duplicates":0,"rxNoRoom":0,"datagramsLost":0,"datagramsDropped":0,"ups":0,"resets":{"total":0,"peerRestarted":0,"retryLimit":0,"protocolError":0,"requested":0},"stalls":0,"payloadErrors":0}"#
        );
    }
}
