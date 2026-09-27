//! The receiver's side toward the application: reassembles in-order fragments
//! into messages and queues everything the application will `recv()`
//! (messages, text, link up / reset), in the order it happened.
//!
//! Queued bytes count against the receive budget, which is what the link
//! advertises as its window: an application that stops reading stops the
//! sender, instead of the link dropping data. Every queued event is charged
//! [`EVENT_COST`] on top of its bytes, so even empty messages cannot grow the
//! queue without bound; text that finds no room is dropped and counted by the
//! link. Lifecycle events are always queued (charged, but never refused): a
//! reset the application has not read yet is superseded by a newer one (with
//! the session it opened), so at most three of them stand in a row.
//!
//! Messages are reassembled per channel: the sender sends the lowest channel
//! first, so a control message can arrive between two fragments of a proto
//! one. Within a channel, fragments arrive in order.
//!
//! Allocation: a delivered message is copied out of its channel's reassembly
//! buffer into a `Vec` of exactly its length, the one allocation per message;
//! the reassembly buffer keeps its capacity (never past `max_message`).

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::LinkEvent;

/// Budget charged per queued event, on top of its bytes (an event slot and a
/// `Vec` header, rounded up).
pub const EVENT_COST: usize = 64;

/// One in-order piece of a message.
#[derive(Clone, Copy, Debug)]
pub struct Fragment<'a> {
    pub chan: u8,
    pub first: bool,
    pub fin: bool,
    pub data: &'a [u8],
}

/// A fragment that does not fit its channel's message: a continuation with
/// no message started, or a new message while one is unfinished. A bug, or a
/// corrupted frame that passed the checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolError;

/// Channels (the header's 3-bit field).
const CHANNELS: usize = 8;

pub struct Inbox {
    events: VecDeque<LinkEvent>,
    /// Per channel: the message being reassembled.
    partials: [Vec<u8>; CHANNELS],
    /// Bit `c`: channel `c` is mid-message.
    open: u8,
    ready_bytes: usize,
    budget: usize,
    max_message: usize,
}

impl Inbox {
    pub fn new(budget: usize, max_message: usize) -> Self {
        Inbox {
            events: VecDeque::new(),
            partials: Default::default(),
            open: 0,
            ready_bytes: 0,
            budget,
            max_message,
        }
    }

    /// Worst-case RAM for an inbox of this shape: the budget (queued messages
    /// and text, charged), the event queue at its largest, and a reassembly
    /// buffer per channel that can carry a fragmented message (`partials`).
    ///
    /// Messages and text are queued only while the charge stays within the
    /// budget, so the charge passes it by at most a trailing up, reset, up,
    /// and the queue holds at most `budget / EVENT_COST + 3` events; a
    /// `VecDeque` at most doubles past its peak.
    pub const fn ram_bound(budget: usize, max_message: usize, partials: usize) -> usize {
        let events = budget / EVENT_COST + 3;
        budget + 3 * EVENT_COST + 2 * events * size_of::<LinkEvent>() + partials * max_message
    }

    /// RAM held now: queued messages and text (charged), the event queue's
    /// capacity and the reassembly buffers'.
    pub fn ram_bytes(&self) -> usize {
        self.ready_bytes
            + self.events.capacity() * size_of::<LinkEvent>()
            + self.partials.iter().map(Vec::capacity).sum::<usize>()
    }

    /// Room to take `n` more bytes toward a message on `chan` (the part of it
    /// already reassembled included) and queue it. Other channels' partial
    /// messages do not count, so two channels mid-message cannot block each
    /// other.
    pub fn has_room(&self, chan: u8, n: usize) -> bool {
        let partial = self.partials.get(chan as usize).map_or(0, Vec::len);
        self.ready_bytes + partial + n + EVENT_COST <= self.budget
    }

    /// Bytes queued for the application or reassembling (charged).
    pub fn bytes(&self) -> usize {
        self.ready_bytes + self.partials.iter().map(Vec::len).sum::<usize>()
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn push_fragment(&mut self, f: Fragment<'_>) -> Result<(), ProtocolError> {
        let c = f.chan as usize % CHANNELS;
        let bit = 1u8 << c;
        if f.first == (self.open & bit != 0) {
            // A new message mid-message, or a continuation of nothing.
            self.abort_all();
            return Err(ProtocolError);
        }
        if self.partials[c].len() + f.data.len() > self.max_message {
            self.abort_all();
            return Err(ProtocolError);
        }
        if f.first && f.fin {
            self.deliver(f.chan, f.data.to_vec());
            return Ok(());
        }
        self.grow_partial(c, f.data.len());
        self.partials[c].extend_from_slice(f.data);
        self.open |= bit;
        if f.fin {
            let data = self.partials[c].as_slice().to_vec();
            self.abort(f.chan);
            self.deliver(f.chan, data);
        }
        Ok(())
    }

    /// Drop `chan`'s half-reassembled message.
    pub fn abort(&mut self, chan: u8) {
        let c = chan as usize % CHANNELS;
        self.partials[c].clear();
        self.open &= !(1 << c);
    }

    /// Drop every half-reassembled message (link reset, a protocol error, or
    /// a gap without ARQ).
    pub fn abort_all(&mut self) {
        self.partials.iter_mut().for_each(Vec::clear);
        self.open = 0;
    }

    /// A best-effort message; the caller checked [`has_room`](Self::has_room).
    pub fn push_datagram(&mut self, channel: u8, data: &[u8]) {
        self.deliver(channel, data.to_vec());
    }

    /// Console text, if there is room for it; `false` = dropped.
    pub fn push_text(&mut self, text: &[u8]) -> bool {
        if self.ready_bytes + text.len() + EVENT_COST > self.budget {
            return false;
        }
        self.ready_bytes += text.len() + EVENT_COST;
        self.events.push_back(LinkEvent::Text(text.to_vec()));
        true
    }

    /// [`LinkEvent::Up`] or [`LinkEvent::Reset`]. A reset supersedes an
    /// unread reset before it (and the `Up` that followed it): the
    /// application drops its per-link state once either way.
    pub fn push_lifecycle(&mut self, ev: LinkEvent) {
        debug_assert!(matches!(ev, LinkEvent::Up { .. } | LinkEvent::Reset { .. }));
        if matches!(ev, LinkEvent::Reset { .. }) {
            loop {
                let n = self.events.len();
                let superseded = match self.events.back() {
                    Some(LinkEvent::Reset { .. }) => true,
                    Some(LinkEvent::Up { .. }) => {
                        n >= 2 && matches!(self.events[n - 2], LinkEvent::Reset { .. })
                    }
                    _ => false,
                };
                if !superseded {
                    break;
                }
                self.events.pop_back();
                self.ready_bytes -= EVENT_COST;
            }
        }
        self.ready_bytes += EVENT_COST;
        self.events.push_back(ev);
    }

    pub fn pop(&mut self) -> Option<LinkEvent> {
        let ev = self.events.pop_front()?;
        self.ready_bytes -= EVENT_COST;
        if let LinkEvent::Message { data, .. } | LinkEvent::Text(data) = &ev {
            self.ready_bytes -= data.len();
        }
        Some(ev)
    }

    fn deliver(&mut self, channel: u8, data: Vec<u8>) {
        self.ready_bytes += data.len() + EVENT_COST;
        self.events.push_back(LinkEvent::Message { channel, data });
    }

    /// Make room for `n` more bytes in channel `c`'s reassembly buffer:
    /// double, but never past `max_message` (the caller checked the message
    /// fits it).
    fn grow_partial(&mut self, c: usize, n: usize) {
        let partial = &mut self.partials[c];
        let need = partial.len() + n;
        if need > partial.capacity() {
            let to = need.max((2 * partial.capacity()).min(self.max_message));
            partial.reserve_exact(to - partial.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResetReason;
    use alloc::vec;

    #[test]
    fn a_newer_reset_supersedes_unread_ones() {
        let mut inbox = Inbox::new(1024, 256);
        let up = |g| LinkEvent::Up { generation: g };
        let reset = |g| LinkEvent::Reset {
            reason: ResetReason::PeerRestarted,
            generation: g,
        };
        inbox.push_lifecycle(up(0));
        for g in 1..10 {
            inbox.push_lifecycle(reset(g));
            inbox.push_lifecycle(up(g));
        }
        let evs: Vec<_> = core::iter::from_fn(|| inbox.pop()).collect();
        assert_eq!(evs, vec![up(0), reset(9), up(9)]);
    }

    #[test]
    fn delivered_messages_are_exactly_their_length() {
        let mut inbox = Inbox::new(64 * 1024, 4096);
        let frag = |first, fin, data| Fragment {
            chan: 1,
            first,
            fin,
            data,
        };
        inbox.push_fragment(frag(true, false, &[1; 300])).unwrap();
        inbox.push_fragment(frag(false, true, &[2; 10])).unwrap();
        let Some(LinkEvent::Message { data, .. }) = inbox.pop() else {
            panic!("no message");
        };
        assert_eq!((data.len(), data.capacity()), (310, 310));
        assert!(inbox.partials[1].capacity() >= 310, "the buffer is kept");
        assert_eq!(inbox.bytes(), 0);
    }

    #[test]
    fn text_past_the_budget_is_refused() {
        let mut inbox = Inbox::new(2 * EVENT_COST + 20, 256);
        assert!(inbox.push_text(b"0123456789"));
        assert!(inbox.push_text(b"0123456789"));
        assert!(!inbox.push_text(b"x"));
        inbox.pop();
        assert!(inbox.push_text(b"x"));
    }

    #[test]
    fn channels_reassemble_independently() {
        let mut inbox = Inbox::new(64 * 1024, 4096);
        let frag = |chan, first, fin, data| Fragment {
            chan,
            first,
            fin,
            data,
        };
        inbox.push_fragment(frag(1, true, false, b"pro")).unwrap();
        inbox.push_fragment(frag(0, true, false, b"con")).unwrap();
        inbox.push_fragment(frag(0, false, true, b"trol")).unwrap();
        inbox.push_fragment(frag(1, false, true, b"to")).unwrap();
        let msgs: Vec<_> = core::iter::from_fn(|| inbox.pop()).collect();
        let msg = |channel, data: &[u8]| LinkEvent::Message {
            channel,
            data: data.to_vec(),
        };
        assert_eq!(msgs, vec![msg(0, b"control"), msg(1, b"proto")]);
    }

    #[test]
    fn a_fragment_that_fits_no_message_is_a_protocol_error() {
        let mut inbox = Inbox::new(64 * 1024, 4096);
        let frag = |first, fin| Fragment {
            chan: 1,
            first,
            fin,
            data: b"x",
        };
        assert_eq!(inbox.push_fragment(frag(false, true)), Err(ProtocolError));
        inbox.push_fragment(frag(true, false)).unwrap();
        assert_eq!(inbox.push_fragment(frag(true, false)), Err(ProtocolError));
        assert_eq!(inbox.bytes(), 0, "the partial is dropped");
    }
}
