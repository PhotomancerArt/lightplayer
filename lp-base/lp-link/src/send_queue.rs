//! Reliable messages accepted by `send()` and not yet cut into frames: one
//! byte ring, allocated once at the send budget, plus a fixed-capacity queue
//! of message descriptors in ring order.
//!
//! Fragments are taken a frame at a time and copied straight into the
//! transmit window's slot, so a message's bytes are released as it goes out.
//! The lowest-numbered channel goes first, fragment by fragment, so a control
//! message overtakes the rest of a big proto message at the next frame
//! boundary; within a channel, messages keep their order. A message taken out
//! of ring order leaves a hole that is reclaimed once everything before it
//! has gone.
//!
//! An **external** message ([`SendQueue::push_external`]) is queued by length
//! alone: its bytes stay in the caller's buffer, and each fragment is copied
//! from there (a `source` passed to [`SendQueue::take`]) straight into the
//! transmit window's slot. It takes no room in the ring, keeps its place in
//! its channel's order, and there is at most one at a time.

use alloc::vec;
use alloc::vec::Vec;

/// The ring has no room for the message, or the descriptor queue is full
/// (or, for an external message, one is already queued).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueFull;

/// Where an external message's fragments come from: `source(offset, out)`
/// fills `out` with the message's bytes at `offset..offset + out.len()`.
pub type ExternalSource<'a> = &'a mut dyn FnMut(usize, &mut [u8]);

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
    /// Message descriptors in ring order: message `i` (oldest first) is
    /// `msgs[(first + i) % msgs.len()]`, for `i < count`. A plain slice rather
    /// than a `VecDeque`: it never grows, so no growth path is linked.
    msgs: Vec<Pending>,
    first: usize,
    count: usize,
}

/// A queued message: where its bytes start in the ring and how far it has
/// been taken.
#[derive(Clone, Copy, Debug, Default)]
struct Pending {
    chan: u8,
    start: usize,
    len: usize,
    taken: usize,
    /// Bytes of it already released from the ring's span.
    released: usize,
    /// Its first fragment went out (a zero-length message has one fragment).
    started: bool,
    /// Its bytes are the caller's, not the ring's.
    external: bool,
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
            msgs: vec![Pending::default(); max_msgs],
            first: 0,
            count: 0,
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
        (0..self.count).all(|i| self.msgs[self.slot(i)].done())
    }

    pub fn clear(&mut self) {
        self.first = 0;
        self.count = 0;
        self.head = 0;
        self.span = 0;
        self.live = 0;
    }

    pub fn push(&mut self, chan: u8, data: &[u8]) -> Result<(), QueueFull> {
        let cap = self.ring.len();
        if self.count >= self.msgs.len() || cap - self.span < data.len() {
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
        let slot = self.slot(self.count);
        self.msgs[slot] = Pending {
            chan,
            start,
            len: data.len(),
            taken: 0,
            released: 0,
            started: false,
            external: false,
        };
        self.count += 1;
        Ok(())
    }

    /// Queue an external message of `len` bytes on `chan` (see the module
    /// docs). Refused while another external message is not yet fully taken.
    pub fn push_external(&mut self, chan: u8, len: usize) -> Result<(), QueueFull> {
        if self.count >= self.msgs.len() || self.external_untaken() {
            return Err(QueueFull);
        }
        let slot = self.slot(self.count);
        self.msgs[slot] = Pending {
            chan,
            start: 0,
            len,
            taken: 0,
            released: 0,
            started: false,
            external: true,
        };
        self.count += 1;
        Ok(())
    }

    /// An external message has bytes not yet taken: its caller must keep its
    /// buffer.
    pub fn external_untaken(&self) -> bool {
        (0..self.count).any(|i| {
            let m = &self.msgs[self.slot(i)];
            m.external && !m.done()
        })
    }

    /// Withdraw the external message, if none of it has been taken yet.
    /// `Err(())`: its first fragment already went out, so the peer holds part
    /// of it and it cannot be withdrawn without breaking the channel's
    /// message (wait for it, or reset the link). `Ok` when there was none.
    pub fn cancel_external(&mut self) -> Result<(), ()> {
        for i in 0..self.count {
            let s = self.slot(i);
            let m = &mut self.msgs[s];
            if m.external && !m.done() {
                if m.started {
                    return Err(());
                }
                // Done with nothing taken: never sent, released in turn.
                m.len = 0;
                m.started = true;
                self.release();
                return Ok(());
            }
        }
        Ok(())
    }

    /// Take the next fragment, at most `out.len()` bytes, into `out`: from the
    /// oldest unfinished message on the lowest-numbered channel that has one.
    /// An external message's bytes come from `source`; without one, nothing
    /// is taken while an external message is next.
    pub fn take(&mut self, out: &mut [u8], source: Option<ExternalSource<'_>>) -> Option<Taken> {
        let mut pick: Option<usize> = None;
        for i in 0..self.count {
            let s = self.slot(i);
            let m = &self.msgs[s];
            if !m.done() && pick.is_none_or(|p| m.chan < self.msgs[p].chan) {
                pick = Some(s);
            }
        }
        let cap = self.ring.len();
        let m = &mut self.msgs[pick?];
        let n = (m.len - m.taken).min(out.len());
        let at = if cap == 0 {
            0
        } else {
            (m.start + m.taken) % cap
        };
        if m.external {
            source?(m.taken, &mut out[..n]);
        } else {
            let first_part = n.min(cap - at);
            out[..first_part].copy_from_slice(&self.ring[at..at + first_part]);
            out[first_part..n].copy_from_slice(&self.ring[..n - first_part]);
        }
        let taken = Taken {
            chan: m.chan,
            first: !m.started,
            fin: m.taken + n == m.len,
            len: n,
        };
        m.started = true;
        m.taken += n;
        if !m.external {
            self.live -= n;
        }
        self.release();
        Some(taken)
    }

    /// Give back the ring bytes no message needs any more: the front
    /// message's taken bytes, and every finished message at the front.
    fn release(&mut self) {
        let cap = self.ring.len();
        while self.count > 0 {
            let m = &mut self.msgs[self.first];
            let r = if m.external { 0 } else { m.taken - m.released };
            m.released = m.taken;
            self.span -= r;
            if cap != 0 {
                self.head = (self.head + r) % cap;
            }
            if !m.done() {
                break;
            }
            self.first = (self.first + 1) % self.msgs.len();
            self.count -= 1;
        }
        debug_assert!(self.count > 0 || self.span == 0);
    }

    /// Where message `i` (oldest first) lives.
    fn slot(&self, i: usize) -> usize {
        (self.first + i) % self.msgs.len().max(1)
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
        let t = q.take(&mut out, None).unwrap();
        assert_eq!((t.first, t.fin, &out[..t.len]), (true, false, &b"abcd"[..]));
        // Four bytes released: a five-byte message wraps around the end.
        q.push(1, b"ghijk").unwrap();
        let t = q.take(&mut out, None).unwrap();
        assert_eq!((t.first, t.fin, &out[..t.len]), (false, true, &b"ef"[..]));
        let mut got = Vec::new();
        while let Some(t) = q.take(&mut out, None) {
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
        let t = q.take(&mut out, None).unwrap();
        assert_eq!((t.first, t.fin, t.len), (true, true, 0));
        assert!(q.take(&mut out, None).is_none());
    }

    #[test]
    fn a_priority_channel_overtakes_and_its_hole_is_reclaimed() {
        let mut q = SendQueue::new(12, 4);
        let mut out = [0u8; 4];
        q.push(1, b"proto-msg").unwrap();
        q.push(0, b"ctl").unwrap();
        let t = q.take(&mut out, None).unwrap();
        assert_eq!((t.chan, &out[..t.len]), (0, &b"ctl"[..]));
        // The hole is behind the unfinished proto message: not free yet.
        assert_eq!(q.span, 12);
        let mut got = Vec::new();
        while let Some(t) = q.take(&mut out, None) {
            got.extend_from_slice(&out[..t.len]);
        }
        assert_eq!(got, b"proto-msg");
        assert_eq!(q.span, 0);
        assert_eq!(q.count, 0);
    }

    #[test]
    fn the_descriptor_queue_is_bounded() {
        let mut q = SendQueue::new(100, 2);
        q.push(0, b"a").unwrap();
        q.push(0, b"b").unwrap();
        assert_eq!(q.push(0, b"c"), Err(QueueFull));
    }

    #[test]
    fn an_external_message_is_cut_from_its_source_in_channel_order() {
        let mut q = SendQueue::new(16, 4);
        let src: Vec<u8> = (0..10).collect();
        let mut out = [0u8; 4];
        q.push(1, b"ab").unwrap();
        q.push_external(1, src.len()).unwrap();
        q.push(1, b"cd").unwrap();
        assert_eq!(q.push_external(1, 3), Err(QueueFull), "one at a time");
        assert_eq!(q.live_bytes(), 4, "the external bytes are not the ring's");
        let mut source =
            |off: usize, dst: &mut [u8]| dst.copy_from_slice(&src[off..off + dst.len()]);
        let mut got = Vec::new();
        while let Some(t) = q.take(&mut out, Some(&mut source)) {
            got.extend_from_slice(&out[..t.len]);
        }
        let mut want = b"ab".to_vec();
        want.extend_from_slice(&src);
        want.extend_from_slice(b"cd");
        assert_eq!(got, want);
        assert!(!q.external_untaken());
        assert!(q.is_empty());
        assert_eq!((q.span, q.count), (0, 0));
    }

    #[test]
    fn without_a_source_an_external_message_waits_and_a_lower_channel_overtakes() {
        let mut q = SendQueue::new(16, 4);
        let mut out = [0u8; 4];
        q.push_external(1, 6).unwrap();
        assert!(q.take(&mut out, None).is_none(), "no source, nothing taken");
        q.push(0, b"ctl").unwrap();
        let t = q.take(&mut out, None).unwrap();
        assert_eq!((t.chan, &out[..t.len]), (0, &b"ctl"[..]));
        let mut source = |_: usize, dst: &mut [u8]| dst.fill(9);
        let t = q.take(&mut out, Some(&mut source)).unwrap();
        assert_eq!((t.chan, t.first, t.fin, t.len), (1, true, false, 4));
        assert!(q.external_untaken(), "two bytes still to cut");
        assert_eq!(q.cancel_external(), Err(()), "started: cannot withdraw");
        let t = q.take(&mut out, Some(&mut source)).unwrap();
        assert!(t.fin);
        assert!(!q.external_untaken());
        // Once taken it is gone: a new external message is accepted.
        q.push_external(1, 1).unwrap();
    }

    #[test]
    fn an_unstarted_external_message_can_be_withdrawn() {
        let mut q = SendQueue::new(16, 4);
        let mut out = [0u8; 4];
        q.push(1, b"ab").unwrap();
        q.push_external(1, 8).unwrap();
        q.push(1, b"cd").unwrap();
        assert_eq!(q.cancel_external(), Ok(()));
        assert!(!q.external_untaken());
        let mut source = |_: usize, _: &mut [u8]| panic!("withdrawn: never read");
        let mut got = Vec::new();
        while let Some(t) = q.take(&mut out, Some(&mut source)) {
            got.extend_from_slice(&out[..t.len]);
        }
        assert_eq!(got, b"abcd");
        assert_eq!((q.span, q.count), (0, 0));
        assert_eq!(q.cancel_external(), Ok(()), "nothing to withdraw");
    }
}
