//! Selective repeat: the receiver keeps frames that arrive past a gap (up to
//! its window) and says which ones it holds in a 32-bit SACK map next to the
//! cumulative ACK, as L2CAP ERTM's SREJ and TCP's SACK do. The sender resends
//! only the holes: early, when a frame sent *after* a hole has been
//! acknowledged (the RACK idea), or when its timer fires.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::inbox::{Fragment, Inbox, ProtocolError};
use crate::seq_num::{seq_dist, seq_is_behind};
use crate::tx_queue::TxQueue;

/// A frame held past a gap.
struct Held {
    chan: u8,
    first: bool,
    fin: bool,
    data: Vec<u8>,
}

pub struct SelectiveRepeat {
    expected: u8,
    /// `slots[i]` holds `expected + i` (slot 0 is only ever filled while the
    /// application has no room for it).
    slots: VecDeque<Option<Held>>,
    held_bytes: usize,
}

impl SelectiveRepeat {
    fn deliver_slot0(&mut self, inbox: &mut Inbox) -> Option<RxVerdict> {
        let h = self.slots.front()?.as_ref()?;
        if !inbox.has_room(h.data.len()) {
            return Some(RxVerdict::NoRoom);
        }
        let frag = Fragment {
            chan: h.chan,
            first: h.first,
            fin: h.fin,
            data: &h.data,
        };
        let ok = inbox.push_fragment(frag).is_ok();
        self.held_bytes -= h.data.len();
        self.advance();
        Some(if ok {
            RxVerdict::InOrder
        } else {
            RxVerdict::Protocol
        })
    }

    fn advance(&mut self) {
        self.slots.pop_front();
        self.slots.push_back(None);
        self.expected = self.expected.wrapping_add(1);
    }
}

impl Arq for SelectiveRepeat {
    const NAME: &'static str = "selective-repeat";
    const MAX_WINDOW: u8 = 32;

    fn new(rx_window: u8) -> Self {
        let n = rx_window.clamp(1, Self::MAX_WINDOW) as usize;
        SelectiveRepeat {
            expected: 0,
            slots: (0..n).map(|_| None).collect(),
            held_bytes: 0,
        }
    }

    fn reset(&mut self) {
        self.expected = 0;
        self.slots.iter_mut().for_each(|s| *s = None);
        self.held_bytes = 0;
    }

    fn expected(&self) -> u8 {
        self.expected
    }

    fn on_data(&mut self, seq: u8, frag: Fragment<'_>, inbox: &mut Inbox) -> RxVerdict {
        if seq_is_behind(self.expected, seq) {
            return RxVerdict::Duplicate;
        }
        let d = seq_dist(self.expected, seq) as usize;
        if d >= self.slots.len() {
            return RxVerdict::OutOfWindow;
        }
        if d == 0 && self.slots[0].is_none() {
            if !inbox.has_room(frag.data.len()) {
                return RxVerdict::NoRoom;
            }
            if inbox.push_fragment(frag).is_err() {
                return RxVerdict::Protocol;
            }
            self.advance();
            return match self.drain(inbox) {
                Ok(_) => RxVerdict::InOrder,
                Err(ProtocolError) => RxVerdict::Protocol,
            };
        }
        if self.slots[d].is_some() {
            return RxVerdict::Duplicate;
        }
        self.held_bytes += frag.data.len();
        self.slots[d] = Some(Held {
            chan: frag.chan,
            first: frag.first,
            fin: frag.fin,
            data: frag.data.to_vec(),
        });
        RxVerdict::Buffered
    }

    fn drain(&mut self, inbox: &mut Inbox) -> Result<usize, ProtocolError> {
        let mut n = 0;
        loop {
            match self.deliver_slot0(inbox) {
                Some(RxVerdict::InOrder) => n += 1,
                Some(RxVerdict::Protocol) => return Err(ProtocolError),
                _ => return Ok(n),
            }
        }
    }

    fn sack(&self) -> u32 {
        let mut bits = 0u32;
        for (i, s) in self.slots.iter().enumerate().skip(1).take(32) {
            if s.is_some() {
                bits |= 1 << (i - 1);
            }
        }
        bits
    }

    fn reorder_bytes(&self) -> usize {
        self.held_bytes
    }

    fn on_timeout(tx: &mut TxQueue, now: Micros, rto: Micros) {
        for e in tx.iter_mut() {
            if !e.sacked && e.sent_at.is_some_and(|t| t + rto <= now) {
                e.sent_at = None;
            }
        }
    }

    fn on_feedback(tx: &mut TxQueue, fb: Feedback) -> usize {
        if fb.sack == 0 {
            return 0;
        }
        // The cumulative part was applied, so `base` is the receiver's ack.
        let base = tx.base();
        for i in 0..32u8 {
            if fb.sack & (1 << i) != 0
                && let Some(e) = tx.get_mut(base.wrapping_add(1 + i))
            {
                e.sacked = true;
            }
        }
        // A hole is lost once `reorder_threshold` frames sent after it have
        // arrived (RACK's "a later send got through", with a count as the
        // reordering allowance). After a resend its order is newer than
        // every frame held, so the same SACK map cannot trigger it again.
        let mut held: [u32; 32] = [0; 32];
        let mut n_held = 0;
        for e in tx.iter().filter(|e| e.sacked) {
            if n_held < held.len() {
                held[n_held] = e.sent_order;
                n_held += 1;
            }
        }
        let held = &held[..n_held];
        let need = fb.reorder_threshold.max(1) as usize;
        let mut n = 0;
        for e in tx.iter_mut() {
            if e.sacked || e.sent_at.is_none() {
                continue;
            }
            let after = held.iter().filter(|&&o| o > e.sent_order).count();
            if after >= need {
                e.sent_at = None;
                n += 1;
            }
        }
        n
    }
}
