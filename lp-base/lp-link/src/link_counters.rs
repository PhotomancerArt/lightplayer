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
    pub datagrams_dropped: u32,
    /// Partial frames flushed after going quiet (a lost tail).
    pub stale_partials: u32,
    pub text_bytes: u32,
    pub ups: u32,
    pub resets: u32,
    /// SYNs from the current peer that named a nonce we no longer use.
    pub stale_syns: u32,
    pub protocol_errors: u32,
}
