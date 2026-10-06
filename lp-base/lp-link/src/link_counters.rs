//! Everything the link recovered from, counted. A recovery that is invisible
//! is a bug we can no longer see, so every retransmit, rejected frame and
//! reset lands here for the heartbeat (or a test) to report.
//!
//! The secure channel's counters exist only with feature `secure`, so a
//! plain firmware image holds exactly the bytes it held before. They are
//! lp-link-local: no wire heartbeat carries them yet (the first secure
//! product link adds them, with its wire version bump).

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkCounters {
    pub frames_tx: u32,
    pub frames_rx: u32,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    /// Reliable data frames sent, first sends and resends.
    pub data_frames_tx: u32,
    /// Reliable data frames sent again.
    pub retransmits: u32,
    /// Retransmit timer expiries.
    pub timeouts: u32,
    /// Resends triggered early by a NAK or a selective ACK.
    pub fast_retransmits: u32,
    /// Tail-loss probes: the newest frame resent after a quiet flight.
    pub probes: u32,
    /// Frames that failed COBS, header or checksum: damaged on the way.
    pub bad_frames: u32,
    /// Frames that verified only under the previous session's key.
    pub stale_frames: u32,
    /// Frames longer than the maximum, discarded.
    pub oversize_frames: u32,
    /// Reliable messages longer than `max_message` the peer sent, or ones
    /// the heap could not reassemble: dropped, acknowledged, and the session
    /// kept.
    pub oversize_messages: u32,
    /// Frames that arrived before the link was up.
    pub dropped_unsynced: u32,
    pub duplicates: u32,
    pub out_of_order: u32,
    /// Data frames refused because the application was not reading.
    pub rx_no_room: u32,
    /// Best-effort messages refused or discarded locally (queue full, no room).
    pub datagrams_dropped: u32,
    /// Best-effort messages the peer sent that never arrived (sequence gaps).
    pub datagrams_lost: u32,
    /// Partial frames flushed after going quiet (a lost tail).
    pub stale_partials: u32,
    pub text_bytes: u32,
    /// Text bytes dropped because the application was not reading.
    pub text_dropped: u32,
    pub ups: u32,
    pub resets: u32,
    /// SYNs from the current peer that named a nonce we no longer use.
    pub stale_syns: u32,
    pub protocol_errors: u32,
    /// Secure handshakes completed (this end split its keys).
    #[cfg(feature = "secure")]
    pub handshakes: u32,
    /// Responder: keys refused (unknown, wrong, backoff, busy). Initiator:
    /// refusals heard.
    #[cfg(feature = "secure")]
    pub handshake_refusals: u32,
    /// SYNs a secure link would not act on: plain SYNs at a secure
    /// responder, msg1s that failed to verify against an established
    /// session, and SYNs that make no sense from that end.
    #[cfg(feature = "secure")]
    pub secure_syn_ignored: u32,
    /// A plain link heard a secure peer: it will never come up with it.
    #[cfg(feature = "secure")]
    pub secure_required: u32,
    /// Frames that passed their checksum but not their tag (forged, or a
    /// bug): dropped on an ARQ link, a reset on a no-ARQ one.
    #[cfg(feature = "secure")]
    pub seal_failures: u32,
    /// Sealed frames refused as already seen (or too old to tell).
    #[cfg(feature = "secure")]
    pub replays: u32,
    /// No-ARQ links: sealed frames that skipped counters (lost on the way,
    /// or dropped by a relay); each resets the link.
    #[cfg(feature = "secure")]
    pub counter_gaps: u32,
}

impl LinkCounters {
    /// What was counted since `base` (a snapshot taken earlier from the
    /// same link), field by field.
    pub fn since(&self, base: &LinkCounters) -> LinkCounters {
        let mut d = LinkCounters::default();
        self.each(base, &mut d, |a, b, out| *out = a.saturating_sub(b));
        d
    }

    /// `self + other`, field by field (counters summed over a link's lives).
    pub fn plus(&self, other: &LinkCounters) -> LinkCounters {
        let mut d = LinkCounters::default();
        self.each(other, &mut d, |a, b, out| *out = a.saturating_add(b));
        d
    }

    /// Apply `f` to every field of `self` and `other`, into `out`.
    fn each(&self, other: &LinkCounters, out: &mut LinkCounters, f: impl Fn(u64, u64, &mut u64)) {
        macro_rules! fields {
            ($($(#[$m:meta])* $f:ident),* $(,)?) => {
                $(
                    $(#[$m])*
                    {
                        let mut v = 0u64;
                        f(u64::from(self.$f), u64::from(other.$f), &mut v);
                        out.$f = v.try_into().unwrap_or_else(|_| !0);
                    }
                )*
            };
        }
        fields!(
            frames_tx,
            frames_rx,
            bytes_tx,
            bytes_rx,
            data_frames_tx,
            retransmits,
            timeouts,
            fast_retransmits,
            probes,
            bad_frames,
            stale_frames,
            oversize_frames,
            oversize_messages,
            dropped_unsynced,
            duplicates,
            out_of_order,
            rx_no_room,
            datagrams_dropped,
            datagrams_lost,
            stale_partials,
            text_bytes,
            text_dropped,
            ups,
            resets,
            stale_syns,
            protocol_errors,
            #[cfg(feature = "secure")]
            handshakes,
            #[cfg(feature = "secure")]
            handshake_refusals,
            #[cfg(feature = "secure")]
            secure_syn_ignored,
            #[cfg(feature = "secure")]
            secure_required,
            #[cfg(feature = "secure")]
            seal_failures,
            #[cfg(feature = "secure")]
            replays,
            #[cfg(feature = "secure")]
            counter_gaps,
        );
    }
}
