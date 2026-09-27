//! Checks a board's soak stream, byte by byte, and says what went missing.
//!
//! Sans-IO: bytes and a millisecond clock go in; what the reader must write
//! back (the packed opt-in) and a running tally come out. The same verifier
//! reads a live port (`link soak`) and a capture made elsewhere, such as the
//! Chrome raw-reader page (`link soak-verify`), so every reader is judged by
//! one rule.
//!
//! What it counts, per the board's soak protocol (`soak_text`):
//!
//! - **soak frames**: whole (declared length and CRC right), **damaged**
//!   (decoded, but length or CRC wrong: bytes lost or changed that the framing
//!   did not catch), and **missing** (a gap in the sequence);
//! - **torn packed frames** (the reader's scanner could not decode them) and
//!   **desyncs** (whole frames dropped because the learned table lost step
//!   after an earlier tear); a torn `M!` line is one whose JSON does not parse;
//! - **the byte ledger**: each `SOAK-STAT` names the bytes the board's io_task
//!   finished writing (`wire`), counted before that stat's own frame was
//!   written. The bytes this reader saw between the starts of two stat frames
//!   against the difference of their `wire` counts is the loss, measured
//!   without trusting any frame to say how long it was. Log lines that race a
//!   stat's sample move bytes between neighbouring intervals, never in or out
//!   of the total, so the total is exact and one interval may read a few
//!   bytes off.

use lpc_wire::{PackOptIn, WireChunk, WireForm, WireServerMessage, WireStream};
use serde_json::{Value, json};

use super::soak_text::parse_soak;

/// A board's own tally, from one `SOAK-STAT`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BoardStat {
    pub t_ms: u64,
    pub seq: u64,
    pub sent: u64,
    pub fail: u64,
    pub wire: u64,
    pub rx_ok: u64,
    pub rx_bad: u64,
    pub rx_gap: u64,
    /// Where this stat's frame began in the reader's stream.
    pub at: u64,
}

/// The running tally.
#[derive(Debug, Default, Clone)]
pub struct SoakTally {
    pub bytes: u64,
    pub soak_whole: u64,
    pub soak_damaged: u64,
    pub soak_missing: u64,
    pub soak_bytes: u64,
    pub soak_packed: u64,
    pub soak_json: u64,
    pub torn_packed: u64,
    pub torn_json: u64,
    pub desynced: u64,
    pub other_frames: u64,
    pub console_lines: u64,
    pub resync_markers: u64,
    pub stats: Vec<BoardStat>,
    /// Bytes the board says it wrote minus bytes seen, summed over stat
    /// intervals.
    pub ledger_deficit: i64,
    /// Stat intervals where the reader saw fewer bytes than the board wrote.
    pub ledger_short_intervals: u64,
    /// The largest frame index seen (for a run's size spread).
    pub largest_soak: u64,
}

/// One anomaly, for the events log.
pub type SoakEvent = Value;

pub struct SoakVerifier {
    stream: WireStream,
    opt_in: PackOptIn,
    offset: u64,
    next_seq: Option<u32>,
    tally: SoakTally,
    events: Vec<SoakEvent>,
    to_send: Vec<String>,
    last_stat: Option<BoardStat>,
    /// The last five bytes, to spot the board's resync marker
    /// (`00 00 'R' 01 00`, written after a write that failed), which the
    /// scanner consumes silently.
    window: [u8; 5],
}

impl SoakVerifier {
    /// `packed`: ask the board to pack its replies (and ask again whenever it
    /// falls back, as Studio does).
    pub fn new(packed: bool) -> Self {
        Self {
            stream: WireStream::new(),
            opt_in: PackOptIn::new(packed),
            offset: 0,
            next_seq: None,
            tally: SoakTally::default(),
            events: Vec::new(),
            to_send: Vec::new(),
            last_stat: None,
            window: [0xFF; 5],
        }
    }

    /// Feed bytes read at `now_ms`.
    pub fn push(&mut self, bytes: &[u8], now_ms: u64) {
        // One byte at a time, so every chunk the scanner emits has an exact
        // stream offset (its last byte).
        for &b in bytes {
            self.offset += 1;
            self.window.rotate_left(1);
            self.window[4] = b;
            if self.window == [0, 0, b'R', 1, 0] {
                self.tally.resync_markers += 1;
                self.event(json!({"kind": "resync-marker", "end": self.offset}));
            }
            let mut chunks = Vec::new();
            self.stream.push(&[b], |c| chunks.push(c));
            for chunk in chunks {
                self.on_chunk(chunk, now_ms);
            }
        }
        self.tally.bytes = self.offset;
    }

    /// The soak restarted (a new `SOAK!` line): sequence from 0.
    pub fn restart_sequence(&mut self) {
        self.next_seq = None;
        self.last_stat = None;
    }

    /// Lines the reader must write to the board, in order (`M!…\n`).
    pub fn take_to_send(&mut self) -> Vec<String> {
        std::mem::take(&mut self.to_send)
    }

    pub fn take_events(&mut self) -> Vec<SoakEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn tally(&self) -> &SoakTally {
        &self.tally
    }

    fn on_chunk(&mut self, chunk: WireChunk, now_ms: u64) {
        let end = self.offset;
        match chunk {
            WireChunk::Line(line) => {
                if !line.is_empty() {
                    self.tally.console_lines += 1;
                }
                if line.starts_with("M!") {
                    // A JSON line the scanner handed back as text: torn.
                    self.tally.torn_json += 1;
                    self.event(json!({"kind": "torn-json-line", "end": end, "len": line.len()}));
                }
            }
            WireChunk::Frame(frame) => {
                let (packed, write_len) = match frame.form {
                    // `\n` + frame
                    WireForm::Packed { wire_len } => (true, wire_len as u64 + 1),
                    // `\n` + `M!` + json + `\n`
                    WireForm::Json => (false, frame.json.len() as u64 + 4),
                };
                let start = end.saturating_sub(write_len);
                let message: WireServerMessage = match lpc_wire::json::from_str(&frame.json) {
                    Ok(m) => m,
                    Err(e) => {
                        self.tally.torn_json += 1;
                        self.event(json!({"kind": "torn-json", "start": start, "end": end,
                            "len": frame.json.len(), "error": e.to_string()}));
                        return;
                    }
                };
                let step = self.opt_in.observe(&message, packed, now_ms);
                if let Some(req) = step.send {
                    self.send(&req);
                }
                if !step.deliver {
                    return;
                }
                self.on_message(message, packed, start, end);
            }
            WireChunk::Error(error) => {
                self.tally.torn_packed += 1;
                self.event(json!({"kind": "torn-packed", "end": end, "detail": error}));
            }
            WireChunk::Desync(d) => {
                self.tally.desynced += 1;
                self.event(json!({"kind": "desync", "end": end, "detail": format!("{d:?}")}));
                if let Some(req) = self.opt_in.desynced(now_ms) {
                    self.send(&req);
                }
            }
        }
    }

    fn on_message(&mut self, message: WireServerMessage, packed: bool, start: u64, end: u64) {
        let lpc_wire::server::ServerMsgBody::Log { message: text, .. } = &message.msg else {
            self.tally.other_frames += 1;
            return;
        };
        if let Some(rest) = text.strip_prefix("SOAK-STAT ") {
            self.on_stat(rest, start);
            return;
        }
        let Some(frame) = parse_soak(text) else {
            self.tally.other_frames += 1;
            return;
        };
        if packed {
            self.tally.soak_packed += 1;
        } else {
            self.tally.soak_json += 1;
        }
        self.tally.largest_soak = self.tally.largest_soak.max(text.len() as u64);
        if let Some(next) = self.next_seq
            && frame.seq > next
        {
            let missing = u64::from(frame.seq - next);
            self.tally.soak_missing += missing;
            self.event(
                json!({"kind": "soak-missing", "first": next, "count": missing,
                "before": start}),
            );
        }
        self.next_seq = Some(frame.seq + 1);
        if frame.len as usize == text.len() && frame.crc_ok {
            self.tally.soak_whole += 1;
            self.tally.soak_bytes += text.len() as u64;
        } else {
            self.tally.soak_damaged += 1;
            self.event(
                json!({"kind": "soak-damaged", "seq": frame.seq, "start": start,
                "end": end, "declared": frame.len, "got": text.len(), "crc_ok": frame.crc_ok,
                "packed": packed}),
            );
        }
    }

    fn on_stat(&mut self, rest: &str, start: u64) {
        let mut stat = BoardStat {
            at: start,
            ..BoardStat::default()
        };
        for pair in rest.split_ascii_whitespace() {
            let Some((k, v)) = pair.split_once('=') else {
                continue;
            };
            let Ok(v) = v.parse::<u64>() else { continue };
            match k {
                "t" => stat.t_ms = v,
                "seq" => stat.seq = v,
                "sent" => stat.sent = v,
                "fail" => stat.fail = v,
                "wire" => stat.wire = v,
                "rx_ok" => stat.rx_ok = v,
                "rx_bad" => stat.rx_bad = v,
                "rx_gap" => stat.rx_gap = v,
                _ => {}
            }
        }
        if let Some(prev) = self.last_stat {
            let board = stat.wire.wrapping_sub(prev.wire) & 0xFFFF_FFFF;
            let seen = stat.at - prev.at;
            let deficit = board as i64 - seen as i64;
            self.tally.ledger_deficit += deficit;
            if deficit > 0 {
                self.tally.ledger_short_intervals += 1;
            }
            if deficit != 0 {
                self.event(json!({"kind": "ledger", "from": prev.at, "to": stat.at,
                    "board_wrote": board, "reader_saw": seen, "deficit": deficit,
                    "board_t_ms": stat.t_ms}));
            }
        }
        self.last_stat = Some(stat);
        self.tally.stats.push(stat);
    }

    fn send(&mut self, req: &lpc_wire::message::client::ClientMessage) {
        if let Ok(line) = lpc_wire::json::to_serial_line(req) {
            self.to_send.push(line);
        }
    }

    fn event(&mut self, event: Value) {
        self.events.push(event);
    }
}

impl SoakTally {
    /// The run in one line.
    pub fn summary(&self) -> String {
        let board = self.stats.last();
        let (sent, fail, rx_ok, rx_bad, rx_gap) = board
            .map(|s| (s.sent, s.fail, s.rx_ok, s.rx_bad, s.rx_gap))
            .unwrap_or_default();
        format!(
            "{} B read; soak frames {} whole ({} packed, {} json), {} damaged, {} missing \
             (board sent {sent}, {fail} send failures); torn packed {}, torn json {}, desynced {}, \
             resync markers {}; ledger deficit {} B over {} short intervals of {}; \
             echo at board: {rx_ok} ok, {rx_bad} bad, {rx_gap} gap",
            self.bytes,
            self.soak_whole,
            self.soak_packed,
            self.soak_json,
            self.soak_damaged,
            self.soak_missing,
            self.torn_packed,
            self.torn_json,
            self.desynced,
            self.resync_markers,
            self.ledger_deficit,
            self.ledger_short_intervals,
            self.stats.len().saturating_sub(1),
        )
    }

    pub fn to_json(&self) -> Value {
        let board = self.stats.last().copied().unwrap_or_default();
        json!({
            "bytes": self.bytes,
            "soak_whole": self.soak_whole,
            "soak_packed": self.soak_packed,
            "soak_json": self.soak_json,
            "soak_damaged": self.soak_damaged,
            "soak_missing": self.soak_missing,
            "soak_bytes": self.soak_bytes,
            "largest_soak": self.largest_soak,
            "torn_packed": self.torn_packed,
            "torn_json": self.torn_json,
            "desynced": self.desynced,
            "resync_markers": self.resync_markers,
            "other_frames": self.other_frames,
            "console_lines": self.console_lines,
            "ledger_deficit": self.ledger_deficit,
            "ledger_short_intervals": self.ledger_short_intervals,
            "stat_intervals": self.stats.len().saturating_sub(1),
            "board_sent": board.sent,
            "board_fail": board.fail,
            "board_wire": board.wire,
            "echo_rx_ok": board.rx_ok,
            "echo_rx_bad": board.rx_bad,
            "echo_rx_gap": board.rx_gap,
        })
    }

    /// Nothing was lost, torn, damaged or short.
    pub fn clean(&self) -> bool {
        self.soak_damaged == 0
            && self.soak_missing == 0
            && self.torn_packed == 0
            && self.torn_json == 0
            && self.desynced == 0
            && self.ledger_short_intervals == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::ServerMsgBody;
    use lpc_wire::server::api::LogLevel;

    fn log_line(text: &str) -> Vec<u8> {
        let msg = WireServerMessage::new(
            0,
            ServerMsgBody::Log {
                level: LogLevel::Info,
                message: text.to_string(),
            },
        );
        let mut line = b"\n".to_vec();
        line.extend(lpc_wire::json::to_serial_line(&msg).unwrap().into_bytes());
        line
    }

    fn stat(wire: u64) -> Vec<u8> {
        log_line(&format!(
            "SOAK-STAT t=0 on=1 seq=0 sent=0 payload=0 fail=0 wire={wire}"
        ))
    }

    #[test]
    fn a_clean_json_stream_counts_whole_frames_and_balances_the_ledger() {
        let mut v = SoakVerifier::new(false);
        let s0 = stat(1000);
        let a = log_line(&super::super::soak_text::echo_text(0, 100));
        let b = log_line(&super::super::soak_text::echo_text(1, 300));
        let s1 = stat(1000 + (s0.len() + a.len() + b.len()) as u64);
        for part in [&s0, &a, &b, &s1] {
            v.push(part, 0);
        }
        let t = v.tally();
        assert_eq!(t.soak_whole, 2);
        assert_eq!(t.ledger_deficit, 0, "{}", t.summary());
        assert!(t.clean());
    }

    #[test]
    fn a_line_short_of_bytes_is_damage_and_a_ledger_deficit() {
        let mut v = SoakVerifier::new(false);
        let s0 = stat(0);
        let a = log_line(&super::super::soak_text::echo_text(0, 400));
        let s1 = stat((s0.len() + a.len()) as u64);
        v.push(&s0, 0);
        // Ten bytes gone from the middle of the pad: still a JSON string.
        let mut torn = a.clone();
        torn.drain(200..210);
        v.push(&torn, 0);
        v.push(&s1, 0);
        let t = v.tally();
        assert_eq!(t.soak_damaged, 1, "{}", t.summary());
        assert_eq!(t.ledger_deficit, 10);
        assert!(!t.clean());
    }

    #[test]
    fn a_missing_frame_is_a_sequence_gap() {
        let mut v = SoakVerifier::new(false);
        for seq in [0, 1, 3] {
            v.push(&log_line(&super::super::soak_text::echo_text(seq, 64)), 0);
        }
        assert_eq!(v.tally().soak_missing, 1);
    }
}
