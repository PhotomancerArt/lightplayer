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
//! A message longer than `max_message` (a peer with a bigger limit) is dropped
//! and counted, fragment by fragment to its end; the frames are still taken
//! (and so acknowledged) and the session carries on.
//!
//! Allocation: a delivered message is copied out of its channel's reassembly
//! buffer into a `Vec` of exactly its length, the one allocation per message;
//! the reassembly buffer keeps its capacity (never past `max_message`) up to
//! `keep` bytes, and is released once its message is out when it grew past
//! that (`LinkConfig::keep_reassembly`).

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
    /// Bit `c`: channel `c` is mid-way through a message too long to keep.
    dropping: u8,
    /// Messages dropped for being longer than `max_message`, or for a
    /// reassembly buffer the heap could not grow.
    oversize: u32,
    ready_bytes: usize,
    budget: usize,
    max_message: usize,
    /// A reassembly buffer above this capacity is released once its message
    /// is delivered (`LinkConfig::keep_reassembly`).
    keep: usize,
    /// Tests: a heap that cannot grow a reassembly buffer past this.
    #[cfg(test)]
    refuse_growth_past: Option<usize>,
}

impl Inbox {
    pub fn new(budget: usize, max_message: usize, keep: usize) -> Self {
        Inbox {
            events: VecDeque::new(),
            partials: Default::default(),
            open: 0,
            dropping: 0,
            oversize: 0,
            ready_bytes: 0,
            budget,
            max_message,
            keep,
            #[cfg(test)]
            refuse_growth_past: None,
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
        let c = chan as usize % CHANNELS;
        let partial = self.partials[c].len();
        if self.dropping & (1 << c) != 0 || partial + n > self.max_message {
            // It will be dropped, which takes no room.
            return true;
        }
        self.ready_bytes + partial + n + EVENT_COST <= self.budget
    }

    /// Messages dropped so far for being longer than `max_message`, or for
    /// a reassembly buffer the heap could not grow.
    pub fn oversize_messages(&self) -> u32 {
        self.oversize
    }

    /// Bytes queued for the application (charged).
    pub fn ready_bytes(&self) -> usize {
        self.ready_bytes
    }

    /// Bytes queued for the application (charged) or reassembling.
    pub fn bytes(&self) -> usize {
        self.ready_bytes + self.partials.iter().map(Vec::len).sum::<usize>()
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn push_fragment(&mut self, f: Fragment<'_>) -> Result<(), ProtocolError> {
        let c = f.chan as usize % CHANNELS;
        let bit = 1u8 << c;
        if self.dropping & bit != 0 {
            if f.first {
                self.abort_all();
                return Err(ProtocolError);
            }
            if f.fin {
                self.dropping &= !bit;
            }
            return Ok(());
        }
        if f.first == (self.open & bit != 0) {
            // A new message mid-message, or a continuation of nothing.
            self.abort_all();
            return Err(ProtocolError);
        }
        if self.partials[c].len() + f.data.len() > self.max_message {
            self.abort(f.chan);
            self.oversize += 1;
            if !f.fin {
                self.dropping |= bit;
            }
            return Ok(());
        }
        if f.first && f.fin {
            self.deliver(f.chan, f.data.to_vec());
            return Ok(());
        }
        if !self.grow_partial(c, f.data.len()) {
            // The heap cannot hold the message: drop it to its end, as an
            // oversize one is, rather than abort the program. A board's
            // largest free block is often smaller than its `max_message`
            // (PR B's emulated LAN walk: an 8 KB write reset the C6 here,
            // with 13,448 B in one piece).
            self.abort(f.chan);
            self.oversize += 1;
            if !f.fin {
                self.dropping |= bit;
            }
            return Ok(());
        }
        self.partials[c].extend_from_slice(f.data);
        self.open |= bit;
        if f.fin {
            // Past `keep`, the buffer is released right after anyway: hand
            // it over instead of copying it out first, so a large message
            // never briefly needs both the reassembly buffer and its own
            // copy. Within `keep`, the buffer stays allocated for the next
            // message, so it must be copied out of, not taken.
            let data = if self.partials[c].capacity() > self.keep {
                core::mem::take(&mut self.partials[c])
            } else {
                self.partials[c].as_slice().to_vec()
            };
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
        self.dropping &= !(1 << c);
    }

    /// Drop every half-reassembled message (link reset, a protocol error, or
    /// a gap without ARQ).
    pub fn abort_all(&mut self) {
        self.partials.iter_mut().for_each(Vec::clear);
        self.open = 0;
        self.dropping = 0;
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
    /// Room for `n` more bytes in channel `c`'s reassembly buffer: doubled
    /// (to `max_message`) when the heap has it, else exactly what is needed.
    /// `false`: the heap has neither.
    fn grow_partial(&mut self, c: usize, n: usize) -> bool {
        let partial = &mut self.partials[c];
        let need = partial.len() + n;
        if need <= partial.capacity() {
            return true;
        }
        let to = need.max((2 * partial.capacity()).min(self.max_message));
        #[cfg(test)]
        if self.refuse_growth_past.is_some_and(|limit| need > limit) {
            return false;
        }
        #[cfg(test)]
        let to = match self.refuse_growth_past {
            Some(limit) if to > limit => need,
            _ => to,
        };
        let len = partial.len();
        partial.try_reserve_exact(to - len).is_ok() || partial.try_reserve_exact(need - len).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResetReason;
    use alloc::vec;

    #[test]
    fn a_newer_reset_supersedes_unread_ones() {
        let mut inbox = Inbox::new(1024, 256, 256);
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
        let mut inbox = Inbox::new(64 * 1024, 4096, 4096);
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
        let mut inbox = Inbox::new(2 * EVENT_COST + 20, 256, 256);
        assert!(inbox.push_text(b"0123456789"));
        assert!(inbox.push_text(b"0123456789"));
        assert!(!inbox.push_text(b"x"));
        inbox.pop();
        assert!(inbox.push_text(b"x"));
    }

    #[test]
    fn channels_reassemble_independently() {
        let mut inbox = Inbox::new(64 * 1024, 4096, 4096);
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
        let mut inbox = Inbox::new(64 * 1024, 4096, 4096);
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

    /// A message the heap cannot reassemble is dropped to its end and
    /// counted, never an allocation failure; a shorter one after it fits.
    /// (The C6 reset here on an 8 KB write with 13,448 B in one piece.)
    #[test]
    fn a_message_the_heap_cannot_hold_is_dropped_not_fatal() {
        let mut inbox = Inbox::new(64 * 1024, 16 * 1024, 100);
        inbox.refuse_growth_past = Some(150);
        let frag = |first, fin, n| Fragment {
            chan: 1,
            first,
            fin,
            data: &[7; 60][..n],
        };
        inbox.push_fragment(frag(true, false, 60)).unwrap();
        inbox.push_fragment(frag(false, false, 60)).unwrap();
        // 180 B does not fit the heap: dropped here, and the rest with it.
        inbox.push_fragment(frag(false, false, 60)).unwrap();
        inbox.push_fragment(frag(false, true, 10)).unwrap();
        assert_eq!(inbox.oversize_messages(), 1);
        assert!(inbox.pop().is_none(), "nothing delivered");
        // Doubling past the limit falls back to exactly what is needed.
        inbox.push_fragment(frag(true, false, 60)).unwrap();
        inbox.push_fragment(frag(false, false, 60)).unwrap();
        inbox.push_fragment(frag(false, true, 20)).unwrap();
        assert!(
            matches!(inbox.pop(), Some(LinkEvent::Message { data, .. }) if data.len() == 140),
            "a message the heap can hold is whole"
        );
    }

    #[test]
    fn an_oversize_message_is_dropped_to_its_end_and_counted() {
        let mut inbox = Inbox::new(64 * 1024, 100, 100);
        let frag = |first, fin, n| Fragment {
            chan: 1,
            first,
            fin,
            data: &[7; 60][..n],
        };
        inbox.push_fragment(frag(true, false, 60)).unwrap();
        inbox.push_fragment(frag(false, false, 60)).unwrap();
        inbox.push_fragment(frag(false, false, 60)).unwrap();
        inbox.push_fragment(frag(false, true, 10)).unwrap();
        assert_eq!(inbox.oversize_messages(), 1);
        assert!(inbox.pop().is_none(), "nothing delivered");
        // The next message on the channel is whole.
        inbox.push_fragment(frag(true, true, 5)).unwrap();
        assert!(matches!(inbox.pop(), Some(LinkEvent::Message { .. })));
    }

    #[test]
    fn a_finished_message_past_keep_is_handed_over_without_a_copy() {
        // `keep` is small, so the reassembly buffer is released at `fin`:
        // the delivered message should be that same allocation (its pointer
        // unchanged), not a fresh copy sitting next to it.
        let mut inbox = Inbox::new(64 * 1024, 4096, 64);
        inbox
            .push_fragment(Fragment {
                chan: 1,
                first: true,
                fin: false,
                data: &[7; 200],
            })
            .unwrap();
        let ptr_before = inbox.partials[1].as_ptr();
        let cap_before = inbox.partials[1].capacity();
        assert!(cap_before > 64, "test setup: the buffer must exceed keep");
        // An empty fin fragment: nothing left to grow, so this call cannot
        // reallocate the partial buffer on its own — any difference between
        // `ptr_before` and the delivered data's pointer is the handover.
        inbox
            .push_fragment(Fragment {
                chan: 1,
                first: false,
                fin: true,
                data: &[],
            })
            .unwrap();
        let Some(LinkEvent::Message { data, .. }) = inbox.pop() else {
            panic!("no message");
        };
        assert_eq!(data.len(), 200);
        assert_eq!(data.as_ptr(), ptr_before, "handed over, not copied");
        assert_eq!(inbox.partials[1].capacity(), 0, "the buffer was taken");
    }

    #[test]
    fn a_large_reassembly_buffer_is_released_after_delivery_past_keep() {
        let frag = |first, fin| Fragment {
            chan: 1,
            first,
            fin,
            data: &[7; 200],
        };
        for (keep, kept) in [(64, false), (4096, true)] {
            let mut inbox = Inbox::new(64 * 1024, 4096, keep);
            inbox.push_fragment(frag(true, false)).unwrap();
            inbox.push_fragment(frag(false, false)).unwrap();
            inbox.push_fragment(frag(false, true)).unwrap();
            assert!(
                matches!(inbox.pop(), Some(LinkEvent::Message { data, .. }) if data.len() == 600)
            );
            assert_eq!(inbox.partials[1].capacity() >= 600, kept, "keep {keep}");
            // A small message after it grows only what it needs.
            inbox.push_fragment(frag(true, false)).unwrap();
            inbox.push_fragment(frag(false, true)).unwrap();
            assert!(
                matches!(inbox.pop(), Some(LinkEvent::Message { data, .. }) if data.len() == 400)
            );
        }
    }
}
