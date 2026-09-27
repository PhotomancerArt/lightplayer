//! Reliable messages accepted by `send()` and not yet cut into frames: one
//! byte ring, allocated once at the send budget, plus a fixed-capacity queue
//! of message descriptors in ring order.
//!
//! Fragments are taken a frame at a time and copied straight into the
//! transmit window's slot, so a message's bytes are released as it goes out.
//! Messages are normally taken oldest first; one taken out of turn (a higher
//! priority channel overtaking, see [`SendQueue::take`]) leaves a hole that is
//! reclaimed once everything before it has gone.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

/// The ring has no room for the message, or the descriptor queue is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueFull;

/// One fragment taken from the queue (its bytes were copied out).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Taken {
    pub chan: u8,
    pub first: bool,
    pub fin: bool,
    pub len: usize,
}

pub struct SendQueue {
    ring: Vec<u8>,
    /// Physical index of the oldest byte still held.
    head: usize,
    /// Bytes from `head` to the write position, holes included.
    span: usize,
    /// Bytes not yet taken (what the send budget counts).
    live: usize,
    msgs: VecDeque<Pending>,
    max_msgs: usize,
}

/// A queued message: where its bytes start in the ring and how far it has
/// been taken.
#[derive(Clone, Copy, Debug)]
struct Pending {
    chan: u8,
    start: usize,
    len: usize,
    taken: usize,
    /// Bytes of it already released from the ring's span.
    released: usize,
    /// Its first fragment went out (a zero-length message has one fragment).
    started: bool,
}

impl Pending {
    fn done(&self) -> bool {
        self.started && self.taken == self.len
    }
}

impl SendQueue {
    /// A queue holding up to `bytes` bytes in up to `max_msgs` messages.
    pub fn new(bytes: usize, max_msgs: usize) -> Self {
        SendQueue {
            ring: vec![0; bytes],
            head: 0,
            span: 0,
            live: 0,
            msgs: VecDeque::with_capacity(max_msgs),
            max_msgs,
        }
    }

    /// RAM a queue of this shape holds, whatever it carries.
    pub const fn ram_bound(bytes: usize, max_msgs: usize) -> usize {
        bytes + max_msgs * size_of::<Pending>()
    }

    /// Bytes queued and not yet taken.
    pub fn live_bytes(&self) -> usize {
        self.live
    }

    /// Nothing left to take.
    pub fn is_empty(&self) -> bool {
        self.msgs.iter().all(Pending::done)
    }

    pub fn clear(&mut self) {
        self.msgs.clear();
        self.head = 0;
        self.span = 0;
        self.live = 0;
    }

    pub fn push(&mut self, chan: u8, data: &[u8]) -> Result<(), QueueFull> {
        let cap = self.ring.len();
        if self.msgs.len() >= self.max_msgs || cap - self.span < data.len() {
            return Err(QueueFull);
        }
        let start = if cap == 0 {
            0
        } else {
            (self.head + self.span) % cap
        };
        let first = data.len().min(cap - start);
        self.ring[start..start + first].copy_from_slice(&data[..first]);
        self.ring[..data.len() - first].copy_from_slice(&data[first..]);
        self.span += data.len();
        self.live += data.len();
        self.msgs.push_back(Pending {
            chan,
            start,
            len: data.len(),
            taken: 0,
            released: 0,
            started: false,
        });
        Ok(())
    }

    /// Take the next fragment, at most `out.len()` bytes, into `out`: from the
    /// oldest unfinished message on the first channel of `order` that has one
    /// (`order` = `None`: the oldest unfinished message of any channel).
    pub fn take(&mut self, order: Option<&[u8]>, out: &mut [u8]) -> Option<Taken> {
        let i = match order {
            None => self.msgs.iter().position(|m| !m.done())?,
            Some(order) => order
                .iter()
                .find_map(|&c| self.msgs.iter().position(|m| m.chan == c && !m.done()))?,
        };
        let cap = self.ring.len();
        let m = &mut self.msgs[i];
        let n = (m.len - m.taken).min(out.len());
        let at = if cap == 0 {
            0
        } else {
            (m.start + m.taken) % cap
        };
        let first_part = n.min(cap - at);
        out[..first_part].copy_from_slice(&self.ring[at..at + first_part]);
        out[first_part..n].copy_from_slice(&self.ring[..n - first_part]);
        let taken = Taken {
            chan: m.chan,
            first: !m.started,
            fin: m.taken + n == m.len,
            len: n,
        };
        m.started = true;
        m.taken += n;
        self.live -= n;
        self.release();
        Some(taken)
    }

    /// Give back the ring bytes no message needs any more: the front
    /// message's taken bytes, and every finished message at the front.
    fn release(&mut self) {
        let cap = self.ring.len();
        while let Some(m) = self.msgs.front_mut() {
            let r = m.taken - m.released;
            m.released = m.taken;
            self.span -= r;
            if cap != 0 {
                self.head = (self.head + r) % cap;
            }
            if !m.done() {
                break;
            }
            self.msgs.pop_front();
        }
        debug_assert!(!self.msgs.is_empty() || self.span == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_in_order_and_wraps() {
        let mut q = SendQueue::new(10, 4);
        let mut out = [0u8; 4];
        q.push(1, b"abcdef").unwrap();
        assert_eq!(q.push(1, b"ghijk"), Err(QueueFull), "no room");
        let t = q.take(None, &mut out).unwrap();
        assert_eq!((t.first, t.fin, &out[..t.len]), (true, false, &b"abcd"[..]));
        // Four bytes released: a five-byte message wraps around the end.
        q.push(1, b"ghijk").unwrap();
        let t = q.take(None, &mut out).unwrap();
        assert_eq!((t.first, t.fin, &out[..t.len]), (false, true, &b"ef"[..]));
        let mut got = Vec::new();
        while let Some(t) = q.take(None, &mut out) {
            got.extend_from_slice(&out[..t.len]);
        }
        assert_eq!(got, b"ghijk");
        assert!(q.is_empty());
        assert_eq!((q.live_bytes(), q.span), (0, 0));
    }

    #[test]
    fn a_zero_length_message_is_one_fragment() {
        let mut q = SendQueue::new(8, 4);
        q.push(0, b"").unwrap();
        let mut out = [0u8; 4];
        let t = q.take(None, &mut out).unwrap();
        assert_eq!((t.first, t.fin, t.len), (true, true, 0));
        assert!(q.take(None, &mut out).is_none());
    }

    #[test]
    fn a_priority_channel_overtakes_and_its_hole_is_reclaimed() {
        let mut q = SendQueue::new(12, 4);
        let mut out = [0u8; 4];
        q.push(1, b"proto-msg").unwrap();
        q.push(0, b"ctl").unwrap();
        let t = q.take(Some(&[0, 1]), &mut out).unwrap();
        assert_eq!((t.chan, &out[..t.len]), (0, &b"ctl"[..]));
        // The hole is behind the unfinished proto message: not free yet.
        assert_eq!(q.span, 12);
        let mut got = Vec::new();
        while let Some(t) = q.take(Some(&[0, 1]), &mut out) {
            got.extend_from_slice(&out[..t.len]);
        }
        assert_eq!(got, b"proto-msg");
        assert_eq!(q.span, 0);
        assert!(q.msgs.is_empty());
    }

    #[test]
    fn the_descriptor_queue_is_bounded() {
        let mut q = SendQueue::new(100, 2);
        q.push(0, b"a").unwrap();
        q.push(0, b"b").unwrap();
        assert_eq!(q.push(0, b"c"), Err(QueueFull));
    }
}
