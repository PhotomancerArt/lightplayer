//! The board's half of the comms lab, sans-IO. The edge (an embassy task per
//! pipe) owns the [`Link`] and the pipe, and calls:
//!
//! ```text
//! while board.ready_for_event() { let Some(ev) = link.recv() else break;
//!                                 if let Some(action) = board.on_event(ev) { do it } }
//! board.pump(&mut link, extra)      // replies, the held echo, the stream
//! ```
//!
//! Back-pressure is the link's: an echo that does not fit the send budget is
//! held, and the board stops taking events until it goes out, so the link's
//! advertised window closes and the host slows down. Nothing is dropped.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::board_stats::{BoardStats, counters_kv};
use super::lab_command::LabCommand;
use super::lab_rng::LabRng;
use super::soak_message::{self, SoakStream};
use crate::{Arq, CH_CONTROL, CH_PROTO, Link, LinkEvent, SendError};

/// What only the edge can do, asked for by a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardAction {
    /// Block the executor this long (a long shader compile).
    Stall { ms: u32 },
    /// Panic, so the host sees the raw-text path.
    Panic,
    /// Write `n` log lines of about `len` bytes, each starting with
    /// [`LAB_LOG_MARK`](super::LAB_LOG_MARK) and `i/n`.
    Log { n: u32, len: usize },
}

pub struct LabBoard {
    identity: String,
    pub stats: BoardStats,
    /// Next host → board sequence number.
    expect: u32,
    held_echo: Option<Vec<u8>>,
    replies: VecDeque<String>,
    stream: Option<Streaming>,
    stats_due: bool,
}

struct Streaming {
    next: u32,
    /// 0: until `stop`.
    count: u32,
    min: usize,
    max: usize,
    rng: LabRng,
    /// A message `send` refused as `Full`, to offer again.
    pending: Option<Vec<u8>>,
}

impl LabBoard {
    /// `identity` goes into the `hello` reply (pipe, firmware, heap…).
    pub fn new(identity: impl Into<String>) -> Self {
        LabBoard {
            identity: identity.into(),
            stats: BoardStats::default(),
            expect: 0,
            held_echo: None,
            replies: VecDeque::new(),
            stream: None,
            stats_due: false,
        }
    }

    pub fn set_identity(&mut self, identity: impl Into<String>) {
        self.identity = identity.into();
    }

    /// Take the next event from the link? `false` while an echo waits for
    /// room in the send budget.
    pub fn ready_for_event(&self) -> bool {
        self.held_echo.is_none() && self.replies.len() < 8
    }

    pub fn is_streaming(&self) -> bool {
        self.stream.is_some()
    }

    /// Something is waiting to be sent.
    pub fn has_work(&self) -> bool {
        self.held_echo.is_some()
            || !self.replies.is_empty()
            || self.stream.is_some()
            || self.stats_due
    }

    /// A `stats` reply is owed: the edge builds its `extra` facts for the
    /// next [`pump`](Self::pump) only then.
    pub fn stats_due(&self) -> bool {
        self.stats_due
    }

    /// The edge wrote `n` log lines for a [`BoardAction::Log`].
    pub fn note_logs(&mut self, n: u32) {
        self.stats.logs_written += n;
    }

    pub fn on_event(&mut self, ev: LinkEvent) -> Option<BoardAction> {
        match ev {
            LinkEvent::Up { .. } => {
                self.stats.ups += 1;
                self.new_session();
            }
            LinkEvent::Reset { .. } => {
                self.stats.resets += 1;
                self.new_session();
            }
            LinkEvent::Text(t) => self.stats.text_bytes += t.len() as u32,
            LinkEvent::Message {
                channel: CH_PROTO,
                data,
            } => self.on_soak(data),
            LinkEvent::Message {
                channel: CH_CONTROL,
                data,
            } => return self.on_command(&data),
            LinkEvent::Message { .. } => {}
        }
        None
    }

    /// Hand the link what is waiting, in order: replies, the held echo, then
    /// stream messages until the send budget is full. `extra` is appended to a
    /// `stats` reply (the edge's own facts: heap, uptime).
    pub fn pump<A: Arq>(&mut self, link: &mut Link<A>, extra: &str) {
        if self.stats_due {
            let mut line = String::from("stats ");
            self.stats.write_kv(&mut line);
            line.push(' ');
            counters_kv(link.counters(), &mut line);
            if !extra.is_empty() {
                line.push(' ');
                line.push_str(extra);
            }
            self.replies.push_back(line);
            self.stats_due = false;
        }
        while let Some(reply) = self.replies.front() {
            match link.send(CH_CONTROL, reply.as_bytes()) {
                Ok(()) => {
                    self.replies.pop_front();
                }
                Err(SendError::Full) => {
                    self.stats.send_full += 1;
                    return;
                }
                Err(_) => {
                    self.replies.pop_front();
                }
            }
        }
        if let Some(echo) = self.held_echo.take() {
            match link.send(CH_PROTO, &echo) {
                Ok(()) => self.stats.echoed += 1,
                Err(SendError::Full) => {
                    self.stats.send_full += 1;
                    self.held_echo = Some(echo);
                    return;
                }
                Err(_) => {}
            }
        }
        while let Some(s) = self.stream.as_mut() {
            let msg = match s.pending.take() {
                Some(m) => m,
                None => {
                    let len = s.rng.size(s.min, s.max);
                    soak_message::encode(SoakStream::FromBoard, s.next, len)
                }
            };
            match link.send(CH_PROTO, &msg) {
                Ok(()) => {
                    s.next += 1;
                    self.stats.stream_sent += 1;
                    self.stats.stream_bytes += msg.len() as u64;
                    if s.count != 0 && s.next >= s.count {
                        self.finish_stream();
                    }
                }
                Err(SendError::Full) => {
                    // Kept, so the sizes stay the seed's and nothing is
                    // encoded twice.
                    s.pending = Some(msg);
                    self.stats.send_full += 1;
                    return;
                }
                Err(_) => self.finish_stream(),
            }
        }
    }

    fn new_session(&mut self) {
        self.expect = 0;
        self.held_echo = None;
        self.replies.clear();
        self.stream = None;
        self.stats_due = false;
    }

    fn on_soak(&mut self, mut data: Vec<u8>) {
        let Ok(h) = soak_message::verify(&data) else {
            self.stats.rx_bad += 1;
            return;
        };
        if h.stream != SoakStream::ToBoard {
            self.stats.rx_bad += 1;
            return;
        }
        if h.seq < self.expect {
            self.stats.rx_repeats += 1;
            return;
        }
        if h.seq > self.expect {
            self.stats.rx_gaps += h.seq - self.expect;
        }
        self.expect = h.seq + 1;
        self.stats.rx_ok += 1;
        self.stats.rx_bytes += data.len() as u64;
        soak_message::make_echo(&mut data);
        self.held_echo = Some(data);
    }

    fn on_command(&mut self, data: &[u8]) -> Option<BoardAction> {
        let text = core::str::from_utf8(data).unwrap_or("");
        let Some(cmd) = LabCommand::parse(text) else {
            self.replies
                .push_back(format!("err unknown command `{text}`"));
            return None;
        };
        match cmd {
            LabCommand::Hello => self.replies.push_back(format!("hello {}", self.identity)),
            LabCommand::ResetStats => {
                self.stats = BoardStats::default();
                self.replies.push_back("ok reset-stats".into());
            }
            LabCommand::Stream {
                count,
                min,
                max,
                seed,
            } => {
                self.stream = Some(Streaming {
                    next: 0,
                    count,
                    min,
                    max,
                    rng: LabRng::new(seed),
                    pending: None,
                });
            }
            LabCommand::Stop => {
                if self.stream.is_some() {
                    self.finish_stream();
                } else {
                    self.replies.push_back("stream done sent=0".into());
                }
            }
            LabCommand::Stats => self.stats_due = true,
            LabCommand::Log { n, len } => {
                self.replies.push_back("ok log".into());
                return Some(BoardAction::Log { n, len });
            }
            LabCommand::Stall { ms } => {
                self.replies.push_back(format!("ok stall {ms}"));
                return Some(BoardAction::Stall { ms });
            }
            LabCommand::Panic => return Some(BoardAction::Panic),
        }
        None
    }

    fn finish_stream(&mut self) {
        if let Some(s) = self.stream.take() {
            self.replies
                .push_back(format!("stream done sent={}", s.next));
        }
    }
}
