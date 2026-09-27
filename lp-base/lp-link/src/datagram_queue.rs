//! Best-effort messages (logs) waiting to go out: a fixed number of slots of
//! one frame's payload each, allocated once. Oldest first; a full queue
//! refuses, it never grows.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

pub struct DatagramQueue {
    /// `(channel, length)` of each queued datagram, oldest first; datagram
    /// `i` is in slot `(head + i) % slots`.
    queued: VecDeque<(u8, u16)>,
    slab: Vec<u8>,
    slot_len: usize,
    slots: usize,
    head: usize,
    bytes: usize,
}

impl DatagramQueue {
    pub fn new(slots: usize, slot_len: usize) -> Self {
        DatagramQueue {
            queued: VecDeque::with_capacity(slots),
            slab: vec![0; slots * slot_len],
            slot_len,
            slots,
            head: 0,
            bytes: 0,
        }
    }

    /// RAM a queue of this shape holds.
    pub const fn ram_bound(slots: usize, slot_len: usize) -> usize {
        slots * (slot_len + size_of::<(u8, u16)>())
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.queued.len() >= self.slots
    }

    /// Payload bytes queued.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn clear(&mut self) {
        self.queued.clear();
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
        let slot = (self.head + self.queued.len()) % self.slots;
        let at = slot * self.slot_len;
        let Some(n) = fill(&mut self.slab[at..at + self.slot_len]) else {
            return false;
        };
        self.bytes += n;
        self.queued.push_back((chan, n as u16));
        true
    }

    /// The oldest datagram: its channel and bytes.
    pub fn front(&self) -> Option<(u8, &[u8])> {
        let &(chan, len) = self.queued.front()?;
        let at = self.head * self.slot_len;
        Some((chan, &self.slab[at..at + len as usize]))
    }

    pub fn pop_front(&mut self) {
        if let Some((_, len)) = self.queued.pop_front() {
            self.bytes -= len as usize;
            self.head = (self.head + 1) % self.slots;
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
