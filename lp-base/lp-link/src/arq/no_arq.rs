//! No retransmission: framing, checksums, channels and the link lifecycle
//! only. Right for a transport that is already reliable and ordered
//! (WebSocket, TCP); on a lossy one it is the baseline that shows what the
//! other variants buy. A gap aborts every message being reassembled (the lost
//! frames could have been any channel's), and each channel skips frames up to
//! its next message start.

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::inbox::{Fragment, Inbox};
use crate::tx_queue::TxQueue;

pub struct NoArq {
    expected: u8,
    /// Bit `c`: channel `c` skips frames until its next message start.
    skipping: u8,
}

impl Arq for NoArq {
    const NAME: &'static str = "no-ARQ";
    const RELIABLE: bool = false;
    const MAX_WINDOW: u8 = 127;

    fn new(_rx_window: u8, _max_payload: usize) -> Self {
        NoArq {
            expected: 0,
            skipping: 0,
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
            inbox.abort_all();
            self.skipping = u8::MAX;
        }
        let bit = 1u8 << (frag.chan & 7);
        if self.skipping & bit != 0 && !frag.first {
            return RxVerdict::Gap;
        }
        self.skipping &= !bit;
        if !inbox.has_room(frag.chan, frag.data.len()) {
            inbox.abort(frag.chan);
            if !frag.fin {
                self.skipping |= bit;
            }
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
