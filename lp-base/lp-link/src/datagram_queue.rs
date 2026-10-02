//! Best-effort messages (logs) waiting to go out: a fixed number of slots of
//! one frame's payload each, allocated once. Oldest first; a full queue
//! refuses, it never grows.

use alloc::vec;
use alloc::vec::Vec;

pub struct DatagramQueue {
    /// `(channel, length)` of the datagram in each slot; datagram `i` (oldest
    /// first) is in slot `(head + i) % slots`, for `i < count`.
    meta: Vec<(u8, u16)>,
    slab: Vec<u8>,
    slot_len: usize,
    slots: usize,
    head: usize,
    count: usize,
    bytes: usize,
}

impl DatagramQueue {
    pub fn new(slots: usize, slot_len: usize) -> Self {
        DatagramQueue {
            meta: vec![(0, 0); slots],
            slab: vec![0; slots * slot_len],
            slot_len,
            slots,
            head: 0,
            count: 0,
            bytes: 0,
        }
    }

    /// RAM a queue of this shape holds.
    pub const fn ram_bound(slots: usize, slot_len: usize) -> usize {
        slots * (slot_len + size_of::<(u8, u16)>())
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn is_full(&self) -> bool {
        self.count >= self.slots
    }

    /// Slots free: datagrams a `push_with` would take right now.
    pub fn free_slots(&self) -> usize {
        self.slots.saturating_sub(self.count)
    }

    /// Payload bytes queued.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn clear(&mut self) {
        self.count = 0;
        self.head = 0;
        self.bytes = 0;
    }

    /// Queue one datagram that `fill` writes into a free slot (handed the
    /// whole slot; returns its length, or `None` to queue nothing). `false`
    /// if the queue is full or `fill` declined.
    pub fn push_with(&mut self, chan: u8, fill: impl FnOnce(&mut [u8]) -> Option<usize>) -> bool {
        if self.is_full() {
            return false;
        }
        let slot = (self.head + self.count) % self.slots;
        let at = slot * self.slot_len;
        let Some(n) = fill(&mut self.slab[at..at + self.slot_len]) else {
            return false;
        };
        self.bytes += n;
        self.meta[slot] = (chan, n as u16);
        self.count += 1;
        true
    }

    /// The oldest datagram: its channel and bytes.
    pub fn front(&self) -> Option<(u8, &[u8])> {
        if self.count == 0 {
            return None;
        }
        let (chan, len) = self.meta[self.head];
        let at = self.head * self.slot_len;
        Some((chan, &self.slab[at..at + len as usize]))
    }

    pub fn pop_front(&mut self) {
        if self.count > 0 {
            self.bytes -= self.meta[self.head].1 as usize;
            self.head = (self.head + 1) % self.slots;
            self.count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_and_full() {
        let mut q = DatagramQueue::new(2, 8);
        let put = |q: &mut DatagramQueue, s: &[u8]| {
            q.push_with(2, |slot| {
                slot[..s.len()].copy_from_slice(s);
                Some(s.len())
            })
        };
        assert!(put(&mut q, b"one"));
        assert!(put(&mut q, b"two!"));
        assert!(!put(&mut q, b"three"), "full");
        assert_eq!(q.front(), Some((2, &b"one"[..])));
        q.pop_front();
        assert!(put(&mut q, b"three"));
        assert_eq!(q.bytes(), 9);
        assert_eq!(q.front(), Some((2, &b"two!"[..])));
        q.pop_front();
        assert_eq!(q.front(), Some((2, &b"three"[..])));
    }
}
