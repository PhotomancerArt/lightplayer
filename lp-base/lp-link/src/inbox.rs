//! The receiver's side toward the application: reassembles in-order fragments
//! into messages and queues everything the application will `recv()`
//! (messages, text, link up / reset), in the order it happened.
//!
//! Queued message bytes count against the receive budget, which is what the
//! link advertises as its window: an application that stops reading stops the
//! sender, instead of the link dropping data.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::LinkEvent;

/// One in-order piece of a message.
#[derive(Clone, Copy, Debug)]
pub struct Fragment<'a> {
    pub chan: u8,
    pub first: bool,
    pub fin: bool,
    pub data: &'a [u8],
}

/// A fragment that does not fit the message being reassembled: a bug or a
/// corrupted frame that passed the checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolError;

pub struct Inbox {
    events: VecDeque<LinkEvent>,
    partial: Vec<u8>,
    partial_chan: Option<u8>,
    ready_bytes: usize,
    budget: usize,
    max_message: usize,
}

impl Inbox {
    pub fn new(budget: usize, max_message: usize) -> Self {
        Inbox {
            events: VecDeque::new(),
            partial: Vec::new(),
            partial_chan: None,
            ready_bytes: 0,
            budget,
            max_message,
        }
    }

    /// Room to queue `n` more message bytes.
    pub fn has_room(&self, n: usize) -> bool {
        self.ready_bytes + n <= self.budget
    }

    /// Queued message bytes the application has not read.
    pub fn ready_bytes(&self) -> usize {
        self.ready_bytes
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Bytes held (queued messages and the partial one).
    pub fn bytes(&self) -> usize {
        self.ready_bytes + self.partial.len()
    }

    pub fn push_fragment(&mut self, f: Fragment<'_>) -> Result<(), ProtocolError> {
        if f.first {
            if self.partial_chan.is_some() {
                self.abort_partial();
                return Err(ProtocolError);
            }
            self.partial_chan = Some(f.chan);
        } else if self.partial_chan != Some(f.chan) {
            self.abort_partial();
            return Err(ProtocolError);
        }
        if self.partial.len() + f.data.len() > self.max_message {
            self.abort_partial();
            return Err(ProtocolError);
        }
        self.partial.extend_from_slice(f.data);
        if f.fin {
            let data = core::mem::take(&mut self.partial);
            self.partial_chan = None;
            self.ready_bytes += data.len();
            self.events.push_back(LinkEvent::Message {
                channel: f.chan,
                data,
            });
        }
        Ok(())
    }

    /// Drop a half-reassembled message (link reset, or a gap without ARQ).
    pub fn abort_partial(&mut self) {
        self.partial.clear();
        self.partial_chan = None;
    }

    /// Mid-message: the next fragment must continue it.
    pub fn in_message(&self) -> bool {
        self.partial_chan.is_some()
    }

    pub fn push_datagram(&mut self, channel: u8, data: &[u8]) {
        self.ready_bytes += data.len();
        self.events.push_back(LinkEvent::Message {
            channel,
            data: data.to_vec(),
        });
    }

    pub fn push_event(&mut self, ev: LinkEvent) {
        self.events.push_back(ev);
    }

    pub fn pop(&mut self) -> Option<LinkEvent> {
        let ev = self.events.pop_front()?;
        if let LinkEvent::Message { data, .. } = &ev {
            self.ready_bytes -= data.len();
        }
        Some(ev)
    }
}
