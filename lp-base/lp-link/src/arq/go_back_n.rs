//! Go-back-N, HDLC style: the receiver takes frames strictly in order and
//! drops anything past a gap, sending a NAK (an ACK naming the frame that
//! revealed the gap); the sender then resends everything from the gap on. No
//! reorder buffer on the receiver, which is why it suits a board short of RAM.
//!
//! Stop-and-wait is the same machine with a window of one.

use crate::Micros;
use crate::arq::{Arq, Feedback, RxVerdict};
use crate::inbox::{Fragment, Inbox};
use crate::seq_num::seq_is_behind;
use crate::tx_queue::TxQueue;

pub struct GoBackN<const MAX: u8> {
    expected: u8,
}

/// One frame in flight at a time.
pub type StopAndWait = GoBackN<1>;

impl<const MAX: u8> Arq for GoBackN<MAX> {
    const NAME: &'static str = if MAX == 1 {
        "stop-and-wait"
    } else {
        "go-back-N"
    };
    const MAX_WINDOW: u8 = MAX;

    fn new(_rx_window: u8, _max_payload: usize) -> Self {
        GoBackN { expected: 0 }
    }

    fn reset(&mut self) {
        self.expected = 0;
    }

    fn expected(&self) -> u8 {
        self.expected
    }

    fn on_data(&mut self, seq: u8, frag: Fragment<'_>, inbox: &mut Inbox) -> RxVerdict {
        if seq == self.expected {
            if !inbox.has_room(frag.data.len()) {
                return RxVerdict::NoRoom;
            }
            if inbox.push_fragment(frag).is_err() {
                return RxVerdict::Protocol;
            }
            self.expected = self.expected.wrapping_add(1);
            RxVerdict::InOrder
        } else if seq_is_behind(self.expected, seq) {
            RxVerdict::Duplicate
        } else {
            RxVerdict::Gap
        }
    }

    fn on_timeout(tx: &mut TxQueue, now: Micros, rto: Micros) {
        let expired = tx
            .front()
            .and_then(|e| e.sent_at)
            .is_some_and(|t| t + rto <= now);
        if expired {
            go_back(tx);
        }
    }

    fn on_feedback(tx: &mut TxQueue, fb: Feedback) -> usize {
        let Some(trigger) = fb.trigger else { return 0 };
        let (Some(front), Some(t)) = (tx.front(), tx.get(trigger)) else {
            return 0;
        };
        let Some(front_at) = front.sent_at else {
            return 0;
        };
        // The NAK is fresh evidence only if the frame that revealed the gap
        // went out after the gap frame's latest send, and that send is not
        // still on its way (a NAK for the old flight must not undo the
        // resend already under way).
        let after = t.sent_order > front.sent_order;
        let resend_settled = front.sends == 1 || front_at + fb.srtt <= fb.now;
        if after && resend_settled {
            go_back(tx)
        } else {
            0
        }
    }
}

/// Mark every sent frame for resending; how many.
fn go_back(tx: &mut TxQueue) -> usize {
    let mut n = 0;
    for e in tx.iter_mut() {
        if e.sent_at.take().is_some() {
            n += 1;
        }
    }
    n
}
