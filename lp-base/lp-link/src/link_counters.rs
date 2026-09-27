//! Everything the link recovered from, counted. A recovery that is invisible
//! is a bug we can no longer see, so every retransmit, rejected frame and
//! reset lands here for the heartbeat (or a test) to report.

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
}

impl LinkCounters {
    /// What was counted since `base` (a snapshot taken earlier from the
    /// same link), field by field.
    pub fn since(&self, base: &LinkCounters) -> LinkCounters {
        macro_rules! d {
            ($($f:ident),*) => {
                LinkCounters { $($f: self.$f.saturating_sub(base.$f)),* }
            };
        }
        d!(
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
            protocol_errors
        )
    }
}
