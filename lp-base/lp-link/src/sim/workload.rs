//! What each side sends during a run.

use crate::Micros;
use crate::sim::sim_rng::SimRng;

#[derive(Clone, Debug)]
pub enum Workload {
    /// The board streams `size`-byte messages to the host as fast as the link
    /// takes them (goodput).
    Bulk { size: usize },
    /// Every `interval` the host sends `up` bytes and the board `down` bytes
    /// (a request and a reply), and the board logs a line every `log_every`
    /// (latency).
    Interactive {
        interval: Micros,
        up: usize,
        down: usize,
        log_every: Micros,
    },
    /// Random messages both ways, sizes up to `max_size` (several fragments),
    /// on both reliable channels, plus logs (property tests).
    Random { mean_gap: Micros, max_size: usize },
}

/// One thing a side wants to send now.
#[derive(Clone, Copy, Debug)]
pub enum Send {
    /// A reliable probe message of this size on this channel.
    Message { channel: u8, size: usize },
    /// A log line.
    Log,
}

/// A side's schedule under a workload.
pub struct WorkloadState {
    workload: Workload,
    board: bool,
    next_at: Option<Micros>,
    next_log_at: Option<Micros>,
    rng: SimRng,
}

impl WorkloadState {
    pub fn new(workload: &Workload, board: bool, rng: SimRng) -> Self {
        let (next_at, next_log_at) = match workload {
            Workload::Bulk { .. } => (board.then_some(0), None),
            Workload::Interactive { log_every, .. } => (Some(0), board.then_some(*log_every)),
            Workload::Random { .. } => (Some(0), board.then_some(0)),
        };
        WorkloadState {
            workload: workload.clone(),
            board,
            next_at,
            next_log_at,
            rng,
        }
    }

    /// The next thing due at `now`, if any. `retry`: the previous one was
    /// refused (`Full`) and will be offered again.
    pub fn due(&mut self, now: Micros) -> Option<Send> {
        if self.next_log_at.is_some_and(|t| t <= now) {
            self.next_log_at = Some(match &self.workload {
                Workload::Interactive { log_every, .. } => now + log_every,
                Workload::Random { mean_gap, .. } => now + self.rng.below(2 * mean_gap + 1),
                Workload::Bulk { .. } => return None,
            });
            return Some(Send::Log);
        }
        if !self.next_at.is_some_and(|t| t <= now) {
            return None;
        }
        Some(match &self.workload {
            Workload::Bulk { size } => Send::Message {
                channel: crate::CH_PROTO,
                size: *size,
            },
            Workload::Interactive { up, down, .. } => Send::Message {
                channel: crate::CH_PROTO,
                size: if self.board { *down } else { *up },
            },
            Workload::Random { max_size, .. } => {
                let channel = if self.rng.chance(0.2) {
                    crate::CH_CONTROL
                } else {
                    crate::CH_PROTO
                };
                let size = 18 + self.rng.below(*max_size as u64) as usize;
                Send::Message { channel, size }
            }
        })
    }

    /// The message offered by [`due`](Self::due) was accepted.
    pub fn accepted(&mut self, now: Micros) {
        self.next_at = match &self.workload {
            Workload::Bulk { .. } => Some(now),
            Workload::Interactive { interval, .. } => Some(now + interval),
            Workload::Random { mean_gap, .. } => Some(now + self.rng.below(2 * mean_gap + 1)),
        };
    }

    /// When something is next due (a refused send is retried on every event).
    pub fn next_at(&self) -> Option<Micros> {
        match (self.next_at, self.next_log_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}
