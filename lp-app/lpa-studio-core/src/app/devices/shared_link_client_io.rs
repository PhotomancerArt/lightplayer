//! An `lpa-client` conversation on a device's SHARED link — the io the
//! card's frame feed speaks through while the model's pump keeps draining
//! the same port.
//!
//! The device model's wire mirror is deliberately lossy (a response body
//! surfaces there as a label), so until now anything that wanted a body
//! borrowed the wire exclusively: pump paused, `LinkBorrow` folded, one
//! reader. That is right for a flash or a push and wrong for a picture that
//! wants to arrive several times a second on a card that must keep folding
//! heartbeats meanwhile.
//!
//! So a conversation here does not borrow. It writes its requests as raw
//! lines (`LinkCommand::SendLine`) with ids minted at or above
//! [`APP_CONVERSATION_ID_BASE`], and the transport's demux classifies every
//! reply by that id BEFORE the mirror: replies in the range come out of the
//! pump as [`LinkEvent::Passthrough`] and land in this link's
//! [`ConversationInbox`] instead of the fold. Everything else on the wire —
//! heartbeats, boot lines, the model's own answers — flows to the fold
//! exactly as before. The model never sees a conversation frame, and a
//! conversation never sees a model frame it did not ask for.
//!
//! # What this io does not do
//!
//! - It does not pause the pump, and it must not be used while a coarse
//!   effect or the editor lens HOLDS the wire: the pump is paused then, so
//!   nothing would ever reach the inbox. The feed checks the borrow first.
//! - It does not close the port: `close` is a no-op, the link belongs to
//!   the model.
//! - It does not correlate: `lpa-client`'s protocol session does, and it
//!   already abandons stray ids, so a straggler from a cancelled pull is a
//!   quiet discard rather than a wrong answer.
//!
//! # Several conversations on one link
//!
//! The card's frame feed and the access controller (reading a device's
//! list, adding this browser's key on a USB connect) can each hold a
//! conversation on the same link at once, and the link has ONE inbox. So
//! every io claims its own slice of the app range
//! ([`CONVERSATION_ID_STRIDE`] ids) when it is made, its client mints ids
//! only there, and `receive` takes only the replies in its slice — another
//! conversation's reply stays in the inbox for its owner instead of being
//! read, and discarded, as a stranger's.
//!
//! [`APP_CONVERSATION_ID_BASE`]: lpa_devices::link::APP_CONVERSATION_ID_BASE
//! [`LinkEvent::Passthrough`]: lpa_devices::link::LinkEvent::Passthrough

use core::time::Duration;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use async_trait::async_trait;
use lpa_client::{ClientIo, LpClient};
use lpa_devices::link::{APP_CONVERSATION_ID_BASE, Link, LinkCommand};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::device_effects::DeviceTimerFuture;

/// How often `receive` re-checks the inbox. The pump drains the wire every
/// 20 ms too, so polling faster would only find an empty queue.
const RECEIVE_POLL: Duration = Duration::from_millis(20);

/// Quiet budget for one reply. The feed's own pull deadline is the outer
/// bound; this keeps a `receive` on a link that stopped answering from
/// waiting forever when no outer deadline is set (a bare `send_request`).
const RESPONSE_BUDGET: Duration = Duration::from_secs(5);

/// Replies in the app range, routed here by the pump (or the lens tap) for
/// the conversations on this link: `(request id, the raw M! line)`.
pub type ConversationInbox = Rc<RefCell<VecDeque<(u32, String)>>>;

/// How many request ids one conversation owns (see the module doc). A
/// long-lived conversation (the frame feed keeps one per connection) mints
/// 16 M requests before it would reach the next slice.
pub const CONVERSATION_ID_STRIDE: u32 = 0x0100_0000;

/// The app range's slices, `APP_CONVERSATION_ID_BASE..0x8000_0000`.
const CONVERSATION_SLICES: u32 = (0x8000_0000 - APP_CONVERSATION_ID_BASE) / CONVERSATION_ID_STRIDE;

thread_local! {
    static NEXT_SLICE: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

/// Claim the next slice of the app range (cycling; 64 slices).
fn claim_slice() -> u32 {
    let slice = NEXT_SLICE.with(|next| {
        let slice = next.get();
        next.set((slice + 1) % CONVERSATION_SLICES);
        slice
    });
    APP_CONVERSATION_ID_BASE + slice * CONVERSATION_ID_STRIDE
}

/// `ClientIo` over one link's shared wire.
pub struct SharedLinkClientIo {
    link: Weak<RefCell<Box<dyn Link>>>,
    inbox: ConversationInbox,
    timer: Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>,
    /// The first id of this conversation's slice.
    first_id: u32,
}

impl SharedLinkClientIo {
    pub(super) fn new(
        link: Weak<RefCell<Box<dyn Link>>>,
        inbox: ConversationInbox,
        timer: Rc<RefCell<dyn FnMut(Duration) -> DeviceTimerFuture>>,
    ) -> Self {
        Self {
            link,
            inbox,
            timer,
            first_id: claim_slice(),
        }
    }

    /// An `lpa-client` over this io whose request ids start in its own
    /// slice of the app range, so the transport routes its replies here and
    /// nothing it mints can collide with the model's ids or another
    /// conversation's.
    pub fn into_client(self) -> LpClient<SharedLinkClientIo> {
        let first = u64::from(self.first_id);
        LpClient::new(self).with_request_ids_from(first)
    }

    fn owns(&self, id: u32) -> bool {
        id.wrapping_sub(self.first_id) < CONVERSATION_ID_STRIDE
    }

    /// Whether the link this io speaks through still exists.
    pub fn is_live(&self) -> bool {
        self.link.upgrade().is_some()
    }
}

#[async_trait(?Send)]
impl ClientIo for SharedLinkClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        debug_assert!(
            msg.id >= u64::from(APP_CONVERSATION_ID_BASE),
            "a shared-link conversation must mint ids in the app range (got {})",
            msg.id
        );
        let Some(link) = self.link.upgrade() else {
            return Err(TransportError::Other("the port is gone".to_string()));
        };
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        // `SendLine` appends the newline; the `M!` marker is ours.
        link.borrow_mut()
            .submit(LinkCommand::SendLine(format!("M!{json}")));
        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = Duration::ZERO;
        loop {
            let next = {
                let mut inbox = self.inbox.borrow_mut();
                inbox
                    .iter()
                    .position(|(id, _)| self.owns(*id))
                    .and_then(|at| inbox.remove(at))
            };
            if let Some((_, line)) = next {
                let json = line.strip_prefix("M!").unwrap_or(&line);
                match lpc_wire::json::from_str::<WireServerMessage>(json) {
                    Ok(message) => return Ok(message),
                    // The demux already decoded this line once to classify
                    // it, so a failure here is a bug, not wire noise — but
                    // it is still not worth a hang.
                    Err(error) => {
                        log::warn!("shared-link conversation: malformed passthrough: {error}");
                        continue;
                    }
                }
            }
            if !self.is_live() {
                return Err(TransportError::Other("the port is gone".to_string()));
            }
            if waited >= RESPONSE_BUDGET {
                return Err(TransportError::Other(format!(
                    "device did not respond within {:.1}s",
                    RESPONSE_BUDGET.as_secs_f64()
                )));
            }
            let sleep = (self.timer.borrow_mut())(RECEIVE_POLL);
            sleep.await;
            waited += RECEIVE_POLL;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // The port belongs to the model's link.
        Ok(())
    }
}

impl Drop for SharedLinkClientIo {
    /// Replies nobody will read again (a straggler after a timeout) leave
    /// with their conversation.
    fn drop(&mut self) {
        let first_id = self.first_id;
        self.inbox
            .borrow_mut()
            .retain(|(id, _)| id.wrapping_sub(first_id) >= CONVERSATION_ID_STRIDE);
    }
}
