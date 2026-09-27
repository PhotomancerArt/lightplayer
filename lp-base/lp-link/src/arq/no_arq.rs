//! No retransmission: framing, checksums, channels and the link lifecycle
//! only. Right for a transport that is already reliable and ordered
//! (WebSocket, TCP); on a lossy one it is the baseline that shows what the
//! other variants buy. A gap aborts the message being reassembled, and frames
//! are skipped up to the next message start.

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::inbox::{Fragment, Inbox};
use crate::tx_queue::TxQueue;

pub struct NoArq {
    expected: u8,
    skipping: bool,
}

impl Arq for NoArq {
    const NAME: &'static str = "no-ARQ";
    const RELIABLE: bool = false;
    const MAX_WINDOW: u8 = 127;

    fn new(_rx_window: u8, _max_payload: usize) -> Self {
        NoArq {
            expected: 0,
            skipping: false,
        }
    }

    fn reset(&mut self) {
        *self = Self::new(0, 0);
    }

    fn expected(&self) -> u8 {
        self.expected
    }

    fn on_data(&mut self, seq: u8, frag: Fragment<'_>, inbox: &mut Inbox) -> RxVerdict {
        let gap = seq != self.expected;
        self.expected = seq.wrapping_add(1);
        if gap {
            inbox.abort_partial();
            self.skipping = true;
        }
        if self.skipping && !frag.first {
            return RxVerdict::Gap;
        }
        self.skipping = false;
        if !inbox.has_room(frag.data.len()) {
            inbox.abort_partial();
            self.skipping = !frag.fin;
            return RxVerdict::NoRoom;
        }
        if inbox.push_fragment(frag).is_err() {
            return RxVerdict::Protocol;
        }
        if gap {
            RxVerdict::Gap
        } else {
            RxVerdict::InOrder
        }
    }

    fn on_timeout(_tx: &mut TxQueue, _now: Micros, _rto: Micros) {}

    fn on_feedback(_tx: &mut TxQueue, _fb: Feedback) -> usize {
        0
    }
}
