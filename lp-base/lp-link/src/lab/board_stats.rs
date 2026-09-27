//! What the board counted, and the `k=v` text both sides use to report
//! counters over the control channel.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::LinkCounters;

/// The board's own tally of the soak (zeroed by `reset-stats`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardStats {
    /// Host → board soak messages that verified and arrived in order.
    pub rx_ok: u32,
    pub rx_bytes: u64,
    /// Messages that failed verification (the link delivered damage).
    pub rx_bad: u32,
    /// Sequence numbers skipped (the link lost a message).
    pub rx_gaps: u32,
    /// Sequence numbers seen again (the link duplicated a message).
    pub rx_repeats: u32,
    pub echoed: u32,
    /// Board → host stream messages handed to the link.
    pub stream_sent: u32,
    pub stream_bytes: u64,
    /// `send` said `Full` (back-pressure, not loss).
    pub send_full: u32,
    pub logs_written: u32,
    pub ups: u32,
    pub resets: u32,
    /// Bytes outside frames (the host's commands never are; nonzero is a
    /// host writing raw text at the board).
    pub text_bytes: u32,
}

impl BoardStats {
    /// Append `k=v` pairs.
    pub fn write_kv(&self, out: &mut String) {
        let _ = write!(
            out,
            "rx_ok={} rx_bytes={} rx_bad={} rx_gaps={} rx_repeats={} echoed={} \
             stream_sent={} stream_bytes={} send_full={} logs_written={} ups={} resets={} \
             text_bytes={}",
            self.rx_ok,
            self.rx_bytes,
            self.rx_bad,
            self.rx_gaps,
            self.rx_repeats,
            self.echoed,
            self.stream_sent,
            self.stream_bytes,
            self.send_full,
            self.logs_written,
            self.ups,
            self.resets,
            self.text_bytes,
        );
    }
}

/// A link's counters as `link.k=v` pairs.
pub fn counters_kv(c: &LinkCounters, out: &mut String) {
    let _ = write!(
        out,
        "link.frames_tx={} link.frames_rx={} link.bytes_tx={} link.bytes_rx={} \
         link.data_frames_tx={} link.retransmits={} link.timeouts={} link.fast_retransmits={} \
         link.probes={} link.bad_frames={} link.stale_frames={} link.oversize_frames={} \
         link.oversize_messages={} link.dropped_unsynced={} link.duplicates={} link.out_of_order={} link.rx_no_room={} \
         link.datagrams_dropped={} link.datagrams_lost={} link.stale_partials={} \
         link.text_bytes={} link.text_dropped={} link.ups={} link.resets={} \
         link.stale_syns={} link.protocol_errors={}",
        c.frames_tx,
        c.frames_rx,
        c.bytes_tx,
        c.bytes_rx,
        c.data_frames_tx,
        c.retransmits,
        c.timeouts,
        c.fast_retransmits,
        c.probes,
        c.bad_frames,
        c.stale_frames,
        c.oversize_frames,
        c.oversize_messages,
        c.dropped_unsynced,
        c.duplicates,
        c.out_of_order,
        c.rx_no_room,
        c.datagrams_dropped,
        c.datagrams_lost,
        c.stale_partials,
        c.text_bytes,
        c.text_dropped,
        c.ups,
        c.resets,
        c.stale_syns,
        c.protocol_errors,
    );
}

/// `k=v` pairs back out of a line (anything else is skipped).
pub fn parse_kv(text: &str) -> Vec<(String, u64)> {
    text.split_ascii_whitespace()
        .filter_map(|t| {
            let (k, v) = t.split_once('=')?;
            Some((k.into(), v.parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_read_back() {
        let c = LinkCounters {
            retransmits: 7,
            bad_frames: 3,
            ..Default::default()
        };
        let mut s = String::from("stats ");
        counters_kv(&c, &mut s);
        let kv = parse_kv(&s);
        assert!(kv.contains(&("link.retransmits".into(), 7)));
        assert!(kv.contains(&("link.bad_frames".into(), 3)));
    }
}
