//! The host's half of the comms lab, sans-IO: one soak run as a plan of
//! phases, verified message by message.
//!
//! ```text
//! WaitUp → Hello → ResetStats → Echo (for echo_for) → EchoDrain
//!        → Stream (for stream_for) → StreamDrain → Logs → Stats → Done
//! ```
//!
//! The edge (lp-cli over a serial port or a socket, the in-process emulator
//! test, a browser page) owns the link and the pipe and calls
//! [`LabHost::on_event`] for every event and [`LabHost::drive`] whenever it
//! services the link. Every soak message is checked in full
//! ([`soak_message::verify_pattern`]): an echo must be the next one owed, and
//! a board stream message must carry the next sequence number. Any damaged,
//! missing, repeated or reordered message, or a link reset, is a finding.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use super::LAB_LOG_MARK;
use super::board_stats::parse_kv;
use super::lab_command::LabCommand;
use super::lab_rng::LabRng;
use super::soak_message::{self, SoakStream};
use crate::{Arq, CH_CONTROL, CH_LOG, CH_PROTO, Link, LinkEvent, Micros, SendError};

/// What one run does.
#[derive(Clone, Debug)]
pub struct LabPlan {
    /// Give up if the link is not up by then.
    pub up_timeout: Micros,
    /// Echo soak messages this long.
    pub echo_for: Micros,
    /// Take a board stream this long.
    pub stream_for: Micros,
    /// Soak message sizes, inclusive, log-uniform.
    pub min: usize,
    pub max: usize,
    /// Echo messages outstanding at once.
    pub in_flight: usize,
    pub seed: u64,
    /// Log lines to ask for (0 = skip the phase), and their length.
    pub logs: u32,
    pub log_len: usize,
    /// One executor stall on the board, mid-echo (0 = none).
    pub stall_ms: u32,
    /// How long a drain or a reply may take before the run fails.
    pub reply_timeout: Micros,
}

impl Default for LabPlan {
    fn default() -> Self {
        LabPlan {
            up_timeout: 10_000_000,
            echo_for: 10_000_000,
            stream_for: 10_000_000,
            min: soak_message::SOAK_MIN,
            max: 16 * 1024,
            in_flight: 4,
            seed: 1,
            logs: 100,
            log_len: 100,
            stall_ms: 0,
            reply_timeout: 30_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabPhase {
    WaitUp,
    Hello,
    ResetStats,
    Echo,
    EchoDrain,
    Stream,
    StreamDrain,
    Logs,
    Stats,
    Done,
    Failed,
}

/// Everything a run found.
#[derive(Clone, Debug, Default)]
pub struct LabReport {
    /// The board's `hello` reply.
    pub hello: String,
    pub echo_msgs: u32,
    /// Bytes each way.
    pub echo_bytes: u64,
    pub echo_micros: Micros,
    pub echo_rtts: Vec<u32>,
    /// Echoes that failed verification or were not the one owed.
    pub echo_errors: u32,
    pub stream_msgs: u32,
    pub stream_bytes: u64,
    pub stream_micros: Micros,
    pub stream_errors: u32,
    /// Stream sequence numbers skipped or repeated.
    pub stream_gaps: u32,
    /// What the board said it sent.
    pub stream_board_sent: Option<u32>,
    pub logs_asked: u32,
    pub lab_logs_rx: u32,
    pub other_logs_rx: u32,
    /// The last few log lines, as text.
    pub log_tail: VecDeque<String>,
    pub text_bytes: u64,
    /// The first bytes of raw text (boot, panics).
    pub text_head: Vec<u8>,
    pub ups: u32,
    pub resets: u32,
    /// Messages outstanding when a reset hit (their fate is unknown).
    pub lost_to_reset: u32,
    pub stall_asked_ms: u32,
    /// The board's `stats` reply, parsed.
    pub board: Vec<(String, u64)>,
    pub failure: Option<String>,
}

pub struct LabHost {
    plan: LabPlan,
    phase: LabPhase,
    phase_at: Micros,
    rng: LabRng,
    report: LabReport,
    next_seq: u32,
    /// Echoes owed: (sequence, length, sent at).
    outstanding: VecDeque<(u32, usize, Micros)>,
    /// An echo request `send` refused as `Full`.
    pending_echo: Option<(u32, Vec<u8>)>,
    /// Commands not yet handed to the link, in order.
    pending_cmds: VecDeque<String>,
    echo_start: Micros,
    stall_sent: bool,
    stream_expect: u32,
    stream_first: Option<Micros>,
    stream_last: Micros,
    last_log_at: Micros,
}

const TEXT_HEAD_MAX: usize = 8 * 1024;
const LOG_TAIL_MAX: usize = 12;
/// After the last asked-for log line, how long to wait for stragglers.
const LOG_QUIET: Micros = 1_000_000;

impl LabHost {
    pub fn new(plan: LabPlan, now: Micros) -> Self {
        LabHost {
            rng: LabRng::new(plan.seed),
            plan,
            phase: LabPhase::WaitUp,
            phase_at: now,
            report: LabReport::default(),
            next_seq: 0,
            outstanding: VecDeque::new(),
            pending_echo: None,
            pending_cmds: VecDeque::new(),
            echo_start: now,
            stall_sent: false,
            stream_expect: 0,
            stream_first: None,
            stream_last: now,
            last_log_at: now,
        }
    }

    pub fn phase(&self) -> LabPhase {
        self.phase
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.phase, LabPhase::Done | LabPhase::Failed)
    }

    pub fn report(&self) -> &LabReport {
        &self.report
    }

    pub fn plan(&self) -> &LabPlan {
        &self.plan
    }

    /// Echo messages still owed.
    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }

    /// The latest time `drive` must run by, for a phase deadline.
    pub fn next_deadline(&self) -> Option<Micros> {
        let t = self.phase_at;
        match self.phase {
            LabPhase::WaitUp => Some(t + self.plan.up_timeout),
            LabPhase::Echo => Some((self.echo_start + self.plan.echo_for).min(
                if self.stall_due_at().is_some() {
                    self.echo_start + self.plan.echo_for / 2
                } else {
                    Micros::MAX
                },
            )),
            LabPhase::Stream => Some(t + self.plan.stream_for),
            LabPhase::Logs => Some((self.last_log_at + LOG_QUIET).min(t + self.plan.reply_timeout)),
            LabPhase::Done | LabPhase::Failed => None,
            _ => Some(t + self.plan.reply_timeout),
        }
    }

    pub fn on_event(&mut self, now: Micros, ev: LinkEvent) {
        match ev {
            LinkEvent::Up { .. } => {
                self.report.ups += 1;
                if self.phase == LabPhase::WaitUp {
                    self.enter(now, LabPhase::Hello);
                    self.pending_cmds.push_back(LabCommand::Hello.to_text());
                }
            }
            LinkEvent::Reset { reason, .. } => {
                self.report.resets += 1;
                self.report.lost_to_reset += self.outstanding.len() as u32;
                self.outstanding.clear();
                if !self.is_finished() {
                    self.fail(
                        now,
                        format!("the link reset ({reason:?}) during {:?}", self.phase),
                    );
                }
            }
            LinkEvent::Text(t) => {
                self.report.text_bytes += t.len() as u64;
                let room = TEXT_HEAD_MAX.saturating_sub(self.report.text_head.len());
                self.report
                    .text_head
                    .extend_from_slice(&t[..t.len().min(room)]);
            }
            LinkEvent::Message { channel, data } => match channel {
                CH_PROTO => self.on_soak(now, &data),
                CH_CONTROL => self.on_reply(now, &data),
                CH_LOG => self.on_log(now, &data),
                _ => {}
            },
        }
    }

    /// Send what the plan wants now, and move phases on time.
    pub fn drive<A: Arq>(&mut self, now: Micros, link: &mut Link<A>) {
        self.flush_commands(now, link);
        if let Some(deadline) = self.timeout_at() {
            if now >= deadline {
                let why = match self.phase {
                    LabPhase::WaitUp => "the link never came up".into(),
                    LabPhase::EchoDrain => {
                        format!("{} echoes never came back", self.outstanding.len())
                    }
                    LabPhase::StreamDrain => format!(
                        "the board stream stopped short: {} of {:?} arrived",
                        self.stream_expect, self.report.stream_board_sent
                    ),
                    p => format!("no reply in {p:?}"),
                };
                self.fail(now, why);
                return;
            }
        }
        match self.phase {
            LabPhase::Echo => {
                if let Some(at) = self.stall_due_at() {
                    if now >= at {
                        self.stall_sent = true;
                        self.report.stall_asked_ms = self.plan.stall_ms;
                        self.command(LabCommand::Stall {
                            ms: self.plan.stall_ms,
                        });
                    }
                }
                if now >= self.echo_start + self.plan.echo_for {
                    self.enter(now, LabPhase::EchoDrain);
                    self.check_echo_drained(now, link);
                    return;
                }
                self.fill_echoes(now, link);
            }
            LabPhase::EchoDrain => self.check_echo_drained(now, link),
            LabPhase::Stream => {
                if now >= self.phase_at + self.plan.stream_for {
                    self.enter(now, LabPhase::StreamDrain);
                    self.command(LabCommand::Stop);
                }
            }
            LabPhase::StreamDrain => self.check_stream_drained(now),
            LabPhase::Logs => {
                let all = self.report.lab_logs_rx >= self.plan.logs;
                if all || now >= self.last_log_at + LOG_QUIET {
                    self.enter(now, LabPhase::Stats);
                    self.command(LabCommand::Stats);
                }
            }
            _ => {}
        }
        self.flush_commands(now, link);
    }

    fn flush_commands<A: Arq>(&mut self, now: Micros, link: &mut Link<A>) {
        while let Some(cmd) = self.pending_cmds.front() {
            match link.send(CH_CONTROL, cmd.as_bytes()) {
                Ok(()) => {
                    self.pending_cmds.pop_front();
                }
                Err(SendError::Full) => return,
                Err(e) => {
                    let why = format!("sending `{cmd}`: {e:?}");
                    self.fail(now, why);
                    return;
                }
            }
        }
    }

    fn timeout_at(&self) -> Option<Micros> {
        let t = self.phase_at;
        match self.phase {
            LabPhase::WaitUp => Some(t + self.plan.up_timeout),
            LabPhase::Hello
            | LabPhase::ResetStats
            | LabPhase::EchoDrain
            | LabPhase::StreamDrain
            | LabPhase::Stats => Some(t + self.plan.reply_timeout),
            _ => None,
        }
    }

    fn stall_due_at(&self) -> Option<Micros> {
        (self.plan.stall_ms > 0 && !self.stall_sent)
            .then_some(self.echo_start + self.plan.echo_for / 2)
    }

    fn fill_echoes<A: Arq>(&mut self, now: Micros, link: &mut Link<A>) {
        while self.outstanding.len() < self.plan.in_flight {
            let (seq, msg) = match self.pending_echo.take() {
                Some(p) => p,
                None => {
                    let len = self.rng.size(self.plan.min, self.plan.max);
                    let seq = self.next_seq;
                    self.next_seq += 1;
                    (seq, soak_message::encode(SoakStream::ToBoard, seq, len))
                }
            };
            match link.send(CH_PROTO, &msg) {
                Ok(()) => self.outstanding.push_back((seq, msg.len(), now)),
                Err(SendError::Full) => {
                    self.pending_echo = Some((seq, msg));
                    return;
                }
                Err(e) => {
                    self.fail(now, format!("sending an echo request: {e:?}"));
                    return;
                }
            }
        }
    }

    fn check_echo_drained<A: Arq>(&mut self, now: Micros, link: &mut Link<A>) {
        if !self.outstanding.is_empty() || self.pending_echo.is_some() {
            if self.pending_echo.is_some() {
                self.fill_echoes(now, link);
            }
            return;
        }
        self.report.echo_micros = now - self.echo_start;
        self.enter(now, LabPhase::Stream);
        let seed = self.plan.seed.wrapping_mul(31).wrapping_add(7);
        self.command(LabCommand::Stream {
            count: 0,
            min: self.plan.min,
            max: self.plan.max,
            seed,
        });
    }

    fn check_stream_drained(&mut self, now: Micros) {
        let Some(sent) = self.report.stream_board_sent else {
            return;
        };
        if self.stream_expect < sent {
            return;
        }
        if let Some(first) = self.stream_first {
            self.report.stream_micros = self.stream_last - first;
        }
        if self.plan.logs > 0 {
            self.enter(now, LabPhase::Logs);
            self.last_log_at = now;
            self.report.logs_asked = self.plan.logs;
            self.command(LabCommand::Log {
                n: self.plan.logs,
                len: self.plan.log_len,
            });
        } else {
            self.enter(now, LabPhase::Stats);
            self.command(LabCommand::Stats);
        }
    }

    /// Queue a command; `drive` hands it to the link in order.
    fn command(&mut self, cmd: LabCommand) {
        self.pending_cmds.push_back(cmd.to_text());
    }

    fn on_soak(&mut self, now: Micros, data: &[u8]) {
        let h = match soak_message::verify_pattern(data) {
            Ok(h) => h,
            Err(_) => {
                match data.get(1) {
                    Some(1) => self.report.stream_errors += 1,
                    _ => self.report.echo_errors += 1,
                }
                return;
            }
        };
        match h.stream {
            SoakStream::Echo => match self.outstanding.front() {
                Some(&(seq, len, sent_at)) if seq == h.seq && len == h.len => {
                    self.outstanding.pop_front();
                    self.report.echo_msgs += 1;
                    self.report.echo_bytes += len as u64;
                    self.report.echo_rtts.push((now - sent_at) as u32);
                }
                _ => self.report.echo_errors += 1,
            },
            SoakStream::FromBoard => {
                if h.seq != self.stream_expect {
                    self.report.stream_gaps += 1;
                }
                if h.seq >= self.stream_expect {
                    self.stream_expect = h.seq + 1;
                    self.report.stream_msgs += 1;
                    self.report.stream_bytes += h.len as u64;
                    self.stream_first.get_or_insert(now);
                    self.stream_last = now;
                }
            }
            SoakStream::ToBoard => self.report.echo_errors += 1,
        }
    }

    fn on_reply(&mut self, now: Micros, data: &[u8]) {
        let text = core::str::from_utf8(data).unwrap_or("");
        let verb = text.split_ascii_whitespace().next().unwrap_or("");
        match (self.phase, verb) {
            (LabPhase::Hello, "hello") => {
                self.report.hello = text["hello".len()..].trim().into();
                self.enter(now, LabPhase::ResetStats);
                self.pending_cmds
                    .push_back(LabCommand::ResetStats.to_text());
            }
            (LabPhase::ResetStats, "ok") if text.contains("reset-stats") => {
                self.enter(now, LabPhase::Echo);
                self.echo_start = now;
            }
            (_, "stream") => {
                let sent = parse_kv(text)
                    .into_iter()
                    .find(|(k, _)| k == "sent")
                    .map(|(_, v)| v as u32);
                self.report.stream_board_sent = sent;
            }
            (LabPhase::Stats, "stats") => {
                self.report.board = parse_kv(text);
                self.enter(now, LabPhase::Done);
            }
            (_, "err") => self.fail(now, format!("the board said: {text}")),
            _ => {}
        }
    }

    fn on_log(&mut self, now: Micros, data: &[u8]) {
        let text = core::str::from_utf8(data.get(1..).unwrap_or(&[])).unwrap_or("?");
        if text.contains(LAB_LOG_MARK) {
            self.report.lab_logs_rx += 1;
            self.last_log_at = now;
        } else {
            self.report.other_logs_rx += 1;
        }
        if self.report.log_tail.len() >= LOG_TAIL_MAX {
            self.report.log_tail.pop_front();
        }
        let level = data.first().copied().unwrap_or(0);
        self.report.log_tail.push_back(format!("[{level}] {text}"));
    }

    fn enter(&mut self, now: Micros, phase: LabPhase) {
        self.phase = phase;
        self.phase_at = now;
    }

    fn fail(&mut self, now: Micros, why: String) {
        if self.report.failure.is_none() {
            self.report.failure = Some(why);
        }
        self.enter(now, LabPhase::Failed);
    }
}

impl LabReport {
    /// A board counter from the `stats` reply.
    pub fn board_value(&self, key: &str) -> Option<u64> {
        self.board.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }

    /// Echo goodput, bytes per second each way.
    pub fn echo_rate(&self) -> u64 {
        rate(self.echo_bytes, self.echo_micros)
    }

    /// Board → host stream goodput, bytes per second.
    pub fn stream_rate(&self) -> u64 {
        rate(self.stream_bytes, self.stream_micros)
    }

    /// The `p`th percentile echo round trip (0–100), in microseconds.
    pub fn rtt_percentile(&self, p: u32) -> u32 {
        if self.echo_rtts.is_empty() {
            return 0;
        }
        let mut v = self.echo_rtts.clone();
        v.sort_unstable();
        let i = ((v.len() - 1) as u64 * u64::from(p.min(100)) / 100) as usize;
        v[i]
    }

    /// Every broken promise, or none: the lab's claim is that a reliable
    /// channel delivered every message whole, once and in order.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if let Some(f) = &self.failure {
            p.push(format!("run failed: {f}"));
        }
        if self.echo_msgs == 0 {
            p.push("no echo came back".into());
        }
        if self.stream_msgs == 0 {
            p.push("no board stream message arrived".into());
        }
        for (what, n) in [
            ("damaged or out-of-order echoes", self.echo_errors),
            ("damaged stream messages", self.stream_errors),
            ("stream sequence gaps", self.stream_gaps),
            ("messages outstanding at a reset", self.lost_to_reset),
            ("link resets", self.resets),
        ] {
            if n > 0 {
                p.push(format!("{n} {what}"));
            }
        }
        for key in ["rx_bad", "rx_gaps", "rx_repeats", "resets"] {
            if let Some(n) = self.board_value(key).filter(|&n| n > 0) {
                p.push(format!("board {key}={n}"));
            }
        }
        p
    }
}

fn rate(bytes: u64, micros: Micros) -> u64 {
    if micros == 0 {
        0
    } else {
        bytes * 1_000_000 / micros
    }
}

impl fmt::Display for LabReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "board: {}", self.hello)?;
        writeln!(
            f,
            "echo:   {} msgs, {} B each way in {} ms = {} B/s each way; rtt p50 {} us, p99 {} us, max {} us; {} errors",
            self.echo_msgs,
            self.echo_bytes,
            self.echo_micros / 1000,
            self.echo_rate(),
            self.rtt_percentile(50),
            self.rtt_percentile(99),
            self.rtt_percentile(100),
            self.echo_errors,
        )?;
        writeln!(
            f,
            "stream: {} msgs ({:?} sent), {} B in {} ms = {} B/s; {} errors, {} gaps",
            self.stream_msgs,
            self.stream_board_sent,
            self.stream_bytes,
            self.stream_micros / 1000,
            self.stream_rate(),
            self.stream_errors,
            self.stream_gaps,
        )?;
        writeln!(
            f,
            "logs:   {} of {} asked-for lines, {} other lines",
            self.lab_logs_rx, self.logs_asked, self.other_logs_rx
        )?;
        writeln!(
            f,
            "link:   {} up, {} resets, {} raw text bytes, stall asked {} ms",
            self.ups, self.resets, self.text_bytes, self.stall_asked_ms
        )?;
        if !self.board.is_empty() {
            write!(f, "board stats:")?;
            for (k, v) in &self.board {
                write!(f, " {k}={v}")?;
            }
            writeln!(f)?;
        }
        let problems = self.problems();
        if problems.is_empty() {
            writeln!(f, "verdict: PASS")
        } else {
            writeln!(f, "verdict: FAIL — {}", problems.join("; "))
        }
    }
}
