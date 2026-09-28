//! The sender's window: every reliable frame sent but not yet acknowledged,
//! oldest first, kept for retransmission. The ARQ variant decides which
//! entries to resend (by marking them unsent); the link sends them.
//!
//! Fixed RAM: one payload slot of `max_payload` bytes per window entry, all
//! allocated in [`TxQueue::new`]. Entry `i` (counting from the oldest) lives
//! in slot `(head + i) % window`.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use crate::Micros;
use crate::seq_num::seq_dist;

/// One reliable frame's send history (its payload is in the window's slot).
pub struct TxEntry {
    pub chan: u8,
    pub first: bool,
    pub fin: bool,
    /// Payload bytes in its slot.
    pub len: u16,
    /// When it was last sent; `None` = (re)send it.
    pub sent_at: Option<Micros>,
    /// Link-wide transmit order of its last send (orders same-instant sends).
    pub sent_order: u32,
    pub sends: u8,
    /// The receiver holds it (selective ACK); never resend.
    pub sacked: bool,
}

/// What a cumulative ACK did.
pub struct Acked {
    pub frames: usize,
    /// Round-trip sample from the newest acknowledged frame sent only once
    /// (Karn's rule: a retransmitted frame gives no sample).
    pub rtt_sample: Option<Micros>,
}

pub struct TxQueue {
    entries: VecDeque<TxEntry>,
    slab: Vec<u8>,
    slot_len: usize,
    window: usize,
    /// Slot of the oldest entry.
    head: usize,
    base: u8,
    bytes: usize,
}

impl TxQueue {
    /// A window of `window` frames of at most `slot_len` payload bytes each.
    pub fn new(window: usize, slot_len: usize) -> Self {
        let window = window.max(1);
        TxQueue {
            entries: VecDeque::with_capacity(window),
            slab: vec![0; window * slot_len],
            slot_len,
            window,
            head: 0,
            base: 0,
            bytes: 0,
        }
    }

    /// RAM the window holds for a `window` × `slot_len` shape.
    pub const fn ram_bound(window: usize, slot_len: usize) -> usize {
        let window = if window == 0 { 1 } else { window };
        window * (slot_len + size_of::<TxEntry>())
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.head = 0;
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

    /// Every slot is taken.
    pub fn is_full(&self) -> bool {
        self.entries.len() == self.window
    }

    /// Payload bytes held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Add a frame whose payload `fill` writes into the free slot: it is handed
    /// the whole slot and returns `(channel, first, fin, payload length)`, or
    /// `None` to add nothing. The new frame's sequence number. The window must
    /// not be full.
    pub fn push_with(
        &mut self,
        fill: impl FnOnce(&mut [u8]) -> Option<(u8, bool, bool, usize)>,
    ) -> Option<u8> {
        debug_assert!(!self.is_full());
        if self.is_full() {
            return None;
        }
        let slot = (self.head + self.entries.len()) % self.window;
        let (chan, first, fin, len) = fill(self.slot_mut(slot))?;
        let seq = self.next_seq();
        self.bytes += len;
        self.entries.push_back(TxEntry {
            chan,
            first,
            fin,
            len: len as u16,
            sent_at: None,
            sent_order: 0,
            sends: 0,
            sacked: false,
        });
        Some(seq)
    }

    /// Drop the oldest entry without an ACK (the no-ARQ baseline).
    pub fn pop_front(&mut self) {
        if let Some(e) = self.entries.pop_front() {
            self.bytes -= e.len as usize;
            self.base = self.base.wrapping_add(1);
            self.head = (self.head + 1) % self.window;
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
            self.bytes -= e.len as usize;
            self.head = (self.head + 1) % self.window;
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

    /// The entry for `seq` and its payload.
    pub fn frame_mut(&mut self, seq: u8) -> Option<(&mut TxEntry, &[u8])> {
        let i = seq_dist(self.base, seq) as usize;
        let e = self.entries.get_mut(i)?;
        let at = (self.head + i) % self.window * self.slot_len;
        let len = e.len as usize;
        Some((e, &self.slab[at..at + len]))
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

    fn slot_mut(&mut self, slot: usize) -> &mut [u8] {
        let at = slot * self.slot_len;
        &mut self.slab[at..at + self.slot_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_stay_with_their_frames_across_the_wrap() {
        let mut tx = TxQueue::new(3, 4);
        let push = |tx: &mut TxQueue, b: u8| {
            tx.push_with(|slot| {
                slot[..2].copy_from_slice(&[b, b]);
                Some((1, true, true, 2))
            })
            .unwrap()
        };
        for round in 0..10u8 {
            let s0 = push(&mut tx, round);
            let s1 = push(&mut tx, round.wrapping_add(100));
            assert_eq!(tx.bytes(), 4);
            assert_eq!(tx.frame_mut(s0).unwrap().1, &[round, round]);
            tx.ack_to(s1, 0).unwrap();
            assert_eq!(tx.frame_mut(s1).unwrap().1, &[round + 100, round + 100]);
            tx.ack_to(s1.wrapping_add(1), 0).unwrap();
            assert!(tx.is_empty());
        }
    }
}
