//! Reliability variants behind one trait. A variant owns the receiver's
//! acceptance rule and decides, on the sender's side, which unacknowledged
//! frames to send again. Everything else (framing, the handshake, windows,
//! timers, ACK timing) is shared by [`Link`](crate::Link).
//!
//! | variant | receiver keeps out-of-order frames | sender resends on loss |
//! |---|---|---|
//! | [`StopAndWait`] | no (window of one) | the one frame |
//! | [`GoBackN`] | no, and NAKs the gap | everything from the gap on |
//! | [`SelectiveRepeat`] | yes, up to its window; ACK carries a SACK map | only the holes |
//! | [`NoArq`] | n/a: framing, channels and lifecycle only | nothing |

mod go_back_n;
mod no_arq;
mod selective_repeat;

pub use go_back_n::{GoBackN, StopAndWait};
pub use no_arq::NoArq;
pub use selective_repeat::SelectiveRepeat;

use crate::Micros;
use crate::inbox::{Fragment, Inbox, ProtocolError};
use crate::tx_queue::TxQueue;

/// What the receiver did with a reliable data frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxVerdict {
    /// The expected frame: delivered (and anything it unblocked).
    InOrder,
    /// Ahead of a gap, kept for later (selective repeat).
    Buffered,
    /// Ahead of a gap, dropped (go-back-N); the sender must go back.
    Gap,
    /// Already delivered or already held.
    Duplicate,
    /// The application is not reading; dropped, will be resent.
    NoRoom,
    /// Beyond the receive window.
    OutOfWindow,
    /// It cannot belong to the message being reassembled.
    Protocol,
}

/// What an ACK told the sender beyond its cumulative number.
#[derive(Clone, Copy, Debug)]
pub struct Feedback {
    /// Bit `i`: the receiver holds `ack + 1 + i`.
    pub sack: u32,
    /// The out-of-order frame that made the receiver ACK (a NAK).
    pub trigger: Option<u8>,
    pub now: Micros,
    pub srtt: Micros,
}

pub trait Arq {
    const NAME: &'static str;
    /// Acknowledges and retransmits at all.
    const RELIABLE: bool = true;
    /// Largest window the variant supports.
    const MAX_WINDOW: u8;

    /// A fresh receiver that takes up to `rx_window` frames past its ack.
    fn new(rx_window: u8) -> Self;
    fn reset(&mut self);

    // Receiver.

    /// The next sequence number it wants (the cumulative ACK).
    fn expected(&self) -> u8;
    fn on_data(&mut self, seq: u8, frag: Fragment<'_>, inbox: &mut Inbox) -> RxVerdict;
    /// Deliver held frames the application now has room for; how many.
    fn drain(&mut self, _inbox: &mut Inbox) -> Result<usize, ProtocolError> {
        Ok(0)
    }
    fn sack(&self) -> u32 {
        0
    }
    /// Bytes held out of order.
    fn reorder_bytes(&self) -> usize {
        0
    }

    // Sender.

    /// The retransmit timer fired at `now`: mark what to resend.
    fn on_timeout(tx: &mut TxQueue, now: Micros, rto: Micros);
    /// An ACK arrived (after the cumulative part was applied): mark what to
    /// resend early; how many.
    fn on_feedback(tx: &mut TxQueue, fb: Feedback) -> usize;
}
