//! A link soak generator for the `soak-link` test variant — never shipped.
//!
//! The question it answers (plan `lp2025/2026-09-26-1720-reliable-device-link`,
//! M1b): **where, and how often, do bytes between the board and a host go
//! missing?** So it measures the product's own write path, not a copy of it:
//! every soak frame is an ordinary unsolicited [`ServerMsgBody::Log`] message
//! handed to the real `ServerTransport::send`, serialized into the real frame
//! buffer as a packed frame or a `M!` JSON line (whichever the link agreed,
//! exactly as a reply would be), and written by the chip's real io_task through
//! the IN-endpoint gate and the `ChunkedWriter`. The server keeps running
//! beside it, heartbeats and all.
//!
//! # The frames (board → host)
//!
//! A soak frame's log text is
//!
//! ```text
//! SOAK s=<seq> n=<len> c=<crc32 hex> p=<pad>
//! ```
//!
//! `seq` counts from 0 each time the soak is switched on, `len` is the whole
//! text's length in bytes, `crc` is CRC-32 (IEEE) over the pad, and the pad is
//! pseudo-random base-64 alphabet text from `seed` and `seq`, sized so the
//! whole text is `len` bytes. Sizes are drawn log-uniformly between `min` and
//! `max`. A reader checks every frame: a missing `seq` is a lost frame, a
//! frame that does not decode is a torn one, a bad `n`/`c` is damage the
//! framing did not catch.
//!
//! Once a second a `SOAK-STAT` log says what the board believes it did:
//! frames and bytes handed to the transport, sends that failed, and
//! **`wire`** — every byte the io_task's writes completed on the link, counted
//! by the io_task itself ([`note_wire_bytes`]). The stat text is built just
//! before its own frame is serialized, so on a link with one writer the bytes
//! a reader sees from the start of one stat frame to the start of the next
//! equal the difference of their `wire` counts, to the byte. That comparison
//! is what tells "the board never wrote it" from "the board wrote it and the
//! host lost it".
//!
//! # Control (host → board)
//!
//! Plain text lines the product ignores (only `M!` lines reach the server), so
//! the io_task hands them here first ([`on_host_line`]):
//!
//! - `SOAK! on=1 min=16 max=16384 rate=0 logs=0 seed=1 budget=20 count=0` —
//!   start (or restart, from seq 0) with these settings; any key may be left
//!   out. `rate` is payload bytes per second (0 = as fast as the link takes
//!   them), `logs=N` adds a console log line after every N frames (the second
//!   writer on the byte stream, as in the product), `budget` is the most
//!   milliseconds one server-loop pass spends sending, and `count` stops after
//!   that many frames (0 = never).
//! - `SOAK! on=0` — stop sending soak frames; the stats keep coming, with
//!   the run's counts, until the next `SOAK! on=1`.
//! - `SOAK> s=<seq> n=<len> c=<crc> p=<pad>` — an echo frame from the host:
//!   the host → board direction. The board checks it and counts it in the
//!   next `SOAK-STAT` (`rx_ok`, `rx_bad`, `rx_gap`).
//!
//! The format is duplicated, with the same test vectors, in `lp-cli`'s
//! `link soak` reader; change both or neither.

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use lpc_shared::transport::{LinkId, ServerTransport};
use lpc_wire::WireServerMessage;
use lpc_wire::server::ServerMsgBody;
use lpc_wire::server::api::LogLevel;

/// How often the board reports what it believes it did.
const STAT_INTERVAL_MS: u64 = 1_000;

/// The largest soak text: under the frame buffer's budget with room for the
/// envelope and the `\nM!` framing.
const MAX_TEXT: u32 = 16_000;

/// The smallest text that holds a header.
const MIN_TEXT: u32 = 48;

static ON: AtomicBool = AtomicBool::new(false);
/// Bumped by every `SOAK!` line; the pump restarts its sequence when it moves.
static GENERATION: AtomicU32 = AtomicU32::new(0);
static MIN: AtomicU32 = AtomicU32::new(16);
static MAX: AtomicU32 = AtomicU32::new(16_384);
static RATE: AtomicU32 = AtomicU32::new(0);
static LOGS: AtomicU32 = AtomicU32::new(0);
static SEED: AtomicU32 = AtomicU32::new(1);
static BUDGET_MS: AtomicU32 = AtomicU32::new(20);
static COUNT: AtomicU32 = AtomicU32::new(0);

/// Bytes the io_task's writes completed on the host link (all writers the
/// io_task owns: server frames, log lines, probes). Wraps at 4 GiB; readers
/// take differences.
static WIRE_BYTES: AtomicU32 = AtomicU32::new(0);

static RX_OK: AtomicU32 = AtomicU32::new(0);
static RX_BAD: AtomicU32 = AtomicU32::new(0);
static RX_GAP: AtomicU32 = AtomicU32::new(0);
static RX_BYTES: AtomicU32 = AtomicU32::new(0);
/// The next echo seq the board expects, plus one (0 = none seen yet).
static RX_NEXT: AtomicU32 = AtomicU32::new(0);

/// The io_task completed a write of `n` bytes to the host link.
#[inline]
pub fn note_wire_bytes(n: usize) {
    WIRE_BYTES.fetch_add(n as u32, Relaxed);
}

/// A line the host sent that is not an `M!` message. Returns whether it was a
/// soak line (and so consumed).
pub fn on_host_line(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix("SOAK! ") {
        configure(rest);
        return true;
    }
    if let Some(rest) = line.strip_prefix("SOAK> ") {
        check_echo(rest);
        return true;
    }
    false
}

fn configure(args: &str) {
    let mut on = true;
    for pair in args.split_ascii_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let Ok(value) = value.parse::<u32>() else {
            continue;
        };
        match key {
            "on" => on = value != 0,
            "min" => MIN.store(value, Relaxed),
            "max" => MAX.store(value, Relaxed),
            "rate" => RATE.store(value, Relaxed),
            "logs" => LOGS.store(value, Relaxed),
            "seed" => SEED.store(value, Relaxed),
            "budget" => BUDGET_MS.store(value.max(1), Relaxed),
            "count" => COUNT.store(value, Relaxed),
            _ => {}
        }
    }
    if !on {
        // Stop, keeping the counts: the stats that follow are the run's
        // final tally.
        ON.store(false, Relaxed);
        return;
    }
    RX_OK.store(0, Relaxed);
    RX_BAD.store(0, Relaxed);
    RX_GAP.store(0, Relaxed);
    RX_BYTES.store(0, Relaxed);
    RX_NEXT.store(0, Relaxed);
    ON.store(true, Relaxed);
    GENERATION.fetch_add(1, Relaxed);
}

fn check_echo(rest: &str) {
    RX_BYTES.fetch_add(rest.len() as u32 + 7, Relaxed);
    let full_len = rest.len() + "SOAK ".len();
    match parse_frame(rest) {
        Some(frame) if frame.len as usize == full_len && frame.crc_ok => {
            RX_OK.fetch_add(1, Relaxed);
            let expected = RX_NEXT.load(Relaxed);
            if expected != 0 && frame.seq + 1 != expected && frame.seq >= expected {
                RX_GAP.fetch_add(frame.seq + 1 - expected, Relaxed);
            }
            RX_NEXT.store(frame.seq + 2, Relaxed);
        }
        _ => {
            RX_BAD.fetch_add(1, Relaxed);
        }
    }
}

/// The pump's own state, kept across server-loop passes.
pub struct SoakPump {
    generation: u32,
    seq: u32,
    started_ms: u64,
    payload_sent: u64,
    frames_sent: u32,
    send_failures: u32,
    last_stat_ms: u64,
}

impl SoakPump {
    pub const fn new() -> Self {
        Self {
            generation: 0,
            seq: 0,
            started_ms: 0,
            payload_sent: 0,
            frames_sent: 0,
            send_failures: 0,
            last_stat_ms: 0,
        }
    }

    /// One server-loop pass: send soak frames while the rate and the pass's
    /// time budget allow, and the stat line when it is due.
    pub async fn pump<T: ServerTransport>(&mut self, transport: &mut T, now_ms: u64) {
        let generation = GENERATION.load(Relaxed);
        if generation != self.generation {
            self.generation = generation;
            self.seq = 0;
            self.started_ms = now_ms;
            self.payload_sent = 0;
            self.frames_sent = 0;
            self.send_failures = 0;
            self.last_stat_ms = 0;
        }
        if generation == 0 {
            return;
        }
        if now_ms.saturating_sub(self.last_stat_ms) >= STAT_INTERVAL_MS {
            self.last_stat_ms = now_ms;
            self.send_stat(transport, now_ms).await;
        }
        if !ON.load(Relaxed) {
            return;
        }
        let pass_start = embassy_time::Instant::now();
        let budget = embassy_time::Duration::from_millis(u64::from(BUDGET_MS.load(Relaxed)));
        let rate = u64::from(RATE.load(Relaxed));
        let count = COUNT.load(Relaxed);
        let logs = LOGS.load(Relaxed);
        loop {
            if count != 0 && self.seq >= count {
                ON.store(false, Relaxed);
                return;
            }
            if rate != 0 {
                let elapsed =
                    now_ms.saturating_sub(self.started_ms) + pass_start.elapsed().as_millis();
                if self.payload_sent * 1000 >= rate * elapsed.max(1) {
                    return;
                }
            }
            if pass_start.elapsed() >= budget {
                return;
            }
            let text = soak_text(
                SEED.load(Relaxed),
                self.seq,
                MIN.load(Relaxed),
                MAX.load(Relaxed),
            );
            let len = text.len() as u64;
            let msg = WireServerMessage::new(
                0,
                ServerMsgBody::Log {
                    level: LogLevel::Info,
                    message: text,
                },
            );
            match transport.send(LinkId::PRIMARY, msg).await {
                Ok(()) => {
                    self.frames_sent += 1;
                    self.payload_sent += len;
                }
                Err(_) => self.send_failures += 1,
            }
            self.seq += 1;
            if logs != 0 && self.seq % logs == 0 {
                log::info!("[soak] interleaved console line after seq={}", self.seq - 1);
            }
        }
    }

    async fn send_stat<T: ServerTransport>(&mut self, transport: &mut T, now_ms: u64) {
        let text = format!(
            "SOAK-STAT t={} on={} seq={} sent={} payload={} fail={} wire={} rx_ok={} rx_bad={} rx_gap={} rx_bytes={} min={} max={} rate={} logs={}",
            now_ms,
            u8::from(ON.load(Relaxed)),
            self.seq,
            self.frames_sent,
            self.payload_sent,
            self.send_failures,
            WIRE_BYTES.load(Relaxed),
            RX_OK.load(Relaxed),
            RX_BAD.load(Relaxed),
            RX_GAP.load(Relaxed),
            RX_BYTES.load(Relaxed),
            MIN.load(Relaxed),
            MAX.load(Relaxed),
            RATE.load(Relaxed),
            LOGS.load(Relaxed),
        );
        let msg = WireServerMessage::new(
            0,
            ServerMsgBody::Log {
                level: LogLevel::Info,
                message: text,
            },
        );
        if transport.send(LinkId::PRIMARY, msg).await.is_err() {
            self.send_failures += 1;
        }
    }
}

impl Default for SoakPump {
    fn default() -> Self {
        Self::new()
    }
}

/// The soak text for `seq`: see the module docs.
pub fn soak_text(seed: u32, seq: u32, min: u32, max: u32) -> String {
    let min = min.clamp(MIN_TEXT, MAX_TEXT);
    let max = max.clamp(min, MAX_TEXT);
    let mut rng = XorShift::new(seed ^ seq.wrapping_mul(0x9E37_79B9));
    let len = log_uniform(&mut rng, min, max) as usize;
    let header_without_crc = format!("SOAK s={seq} n={len} c=");
    // header + 8 hex + " p=" + pad == len
    let pad_len = len.saturating_sub(header_without_crc.len() + 8 + 3);
    let mut pad = String::with_capacity(pad_len);
    for _ in 0..pad_len {
        pad.push(ALPHABET[(rng.next() >> 26) as usize] as char);
    }
    let crc = crc32(pad.as_bytes());
    let mut text = header_without_crc;
    text.reserve(len);
    text.push_str(&format!("{crc:08x} p="));
    text.push_str(&pad);
    text
}

/// A parsed soak frame (the text after `SOAK `).
pub struct SoakFrame {
    pub seq: u32,
    pub len: u32,
    pub crc_ok: bool,
}

/// Parse `s=<seq> n=<len> c=<crc> p=<pad>`.
pub fn parse_frame(rest: &str) -> Option<SoakFrame> {
    let rest = rest.strip_prefix("s=")?;
    let (seq, rest) = rest.split_once(" n=")?;
    let (len, rest) = rest.split_once(" c=")?;
    let (crc, pad) = rest.split_once(" p=")?;
    let crc = u32::from_str_radix(crc, 16).ok()?;
    Some(SoakFrame {
        seq: seq.parse().ok()?,
        len: len.parse().ok()?,
        crc_ok: crc32(pad.as_bytes()) == crc,
    })
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// CRC-32 (IEEE 802.3, reflected, init and xorout `0xFFFF_FFFF`), bitwise:
/// no table, so it costs the image a few dozen bytes.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

struct XorShift(u32);

impl XorShift {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x1234_5678 } else { seed })
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

/// A size in `min..=max`, uniform in its power of two, each power of two
/// equally likely, so small and large frames both get their share.
fn log_uniform(rng: &mut XorShift, min: u32, max: u32) -> u32 {
    if min >= max {
        return min;
    }
    let lo_bit = 31 - min.leading_zeros();
    let hi_bit = 31 - max.leading_zeros();
    let bit = lo_bit + rng.next() % (hi_bit - lo_bit + 1);
    let lo = (1u32 << bit).max(min);
    let hi = ((1u64 << (bit + 1)) - 1).min(u64::from(max)) as u32;
    lo + rng.next() % (hi - lo + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors `lp-cli`'s reader pins too.
    #[test]
    fn crc32_matches_the_ieee_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn a_soak_text_is_its_own_declared_length_and_checks() {
        for seq in 0..200 {
            let text = soak_text(7, seq, 16, 16_384);
            let frame = parse_frame(text.strip_prefix("SOAK ").unwrap()).unwrap();
            assert_eq!(frame.seq, seq);
            assert_eq!(frame.len as usize, text.len(), "seq {seq}");
            assert!(frame.crc_ok);
            assert!(text.len() >= MIN_TEXT as usize && text.len() <= MAX_TEXT as usize);
        }
    }

    #[test]
    fn the_first_soak_text_is_pinned() {
        let text = soak_text(1, 0, 64, 64);
        assert_eq!(text.len(), 64);
        assert_eq!(&text[..27], "SOAK s=0 n=64 c=1218cff2 p=");
    }
}
