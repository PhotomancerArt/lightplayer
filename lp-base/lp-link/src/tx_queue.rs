//! The sender's window: every reliable frame sent but not yet acknowledged,
//! oldest first, kept for retransmission. The ARQ variant decides which
//! entries to resend (by marking them unsent); the link sends them.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::Micros;
use crate::seq_num::seq_dist;

/// One reliable frame's payload and its send history.
pub struct TxEntry {
    pub chan: u8,
    pub first: bool,
    pub fin: bool,
    pub payload: Vec<u8>,
    /// When it was last sent; `None` = (re)send it.
    pub sent_at: Option<Micros>,
    /// Link-wide transmit order of its last send (orders same-instant sends).
    pub sent_order: u32,
    pub sends: u8,
    /// The receiver holds it (selective ACK); never resend.
    pub sacked: bool,
}

impl TxEntry {
    pub fn new(chan: u8, first: bool, fin: bool, payload: Vec<u8>) -> Self {
        TxEntry {
            chan,
            first,
            fin,
            payload,
            sent_at: None,
            sent_order: 0,
            sends: 0,
            sacked: false,
        }
    }
}

/// What a cumulative ACK did.
pub struct Acked {
    pub frames: usize,
    /// Round-trip sample from the newest acknowledged frame sent only once
    /// (Karn's rule: a retransmitted frame gives no sample).
    pub rtt_sample: Option<Micros>,
}

#[derive(Default)]
pub struct TxQueue {
    entries: VecDeque<TxEntry>,
    base: u8,
    bytes: usize,
}

impl TxQueue {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.base = 0;
        self.bytes = 0;
    }

    /// Sequence number of the oldest unacknowledged frame.
    pub fn base(&self) -> u8 {
        self.base
    }

    pub fn next_seq(&self) -> u8 {
        self.base.wrapping_add(self.entries.len() as u8)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Payload bytes held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn push(&mut self, entry: TxEntry) -> u8 {
        let seq = self.next_seq();
        self.bytes += entry.payload.len();
        self.entries.push_back(entry);
        seq
    }

    /// Drop the oldest entry without an ACK (the no-ARQ baseline).
    pub fn pop_front(&mut self) {
        if let Some(e) = self.entries.pop_front() {
            self.bytes -= e.payload.len();
            self.base = self.base.wrapping_add(1);
        }
    }

    /// Everything before `ack` is acknowledged. `None` if `ack` is not in
    /// `[base, next_seq]` (stale or garbage).
    pub fn ack_to(&mut self, ack: u8, now: Micros) -> Option<Acked> {
        let n = seq_dist(self.base, ack) as usize;
        if n > self.entries.len() {
            return None;
        }
        let mut rtt_sample = None;
        for _ in 0..n {
            let e = self.entries.pop_front()?;
            self.bytes -= e.payload.len();
            if let (1, Some(t)) = (e.sends, e.sent_at) {
                rtt_sample = Some(now.saturating_sub(t));
            }
        }
        self.base = ack;
        Some(Acked {
            frames: n,
            rtt_sample,
        })
    }

    pub fn get(&self, seq: u8) -> Option<&TxEntry> {
        self.entries.get(seq_dist(self.base, seq) as usize)
    }

    pub fn get_mut(&mut self, seq: u8) -> Option<&mut TxEntry> {
        self.entries.get_mut(seq_dist(self.base, seq) as usize)
    }

    pub fn front(&self) -> Option<&TxEntry> {
        self.entries.front()
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut TxEntry> {
        self.entries.iter_mut()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &TxEntry> {
        self.entries.iter()
    }

    /// The oldest entry waiting to be (re)sent.
    pub fn first_unsent(&self) -> Option<u8> {
        let i = self
            .entries
            .iter()
            .position(|e| e.sent_at.is_none() && !e.sacked)?;
        Some(self.base.wrapping_add(i as u8))
    }

    /// Earliest retransmit deadline among sent, unacknowledged entries.
    pub fn next_timer(&self, rto: Micros) -> Option<Micros> {
        self.entries
            .iter()
            .filter(|e| !e.sacked)
            .filter_map(|e| e.sent_at)
            .min()
            .map(|t| t + rto)
    }
}
