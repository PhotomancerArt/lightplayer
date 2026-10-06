//! Channel 3 — the over-the-air update protocol (`lpc-update`, protocol v1)
//! — on the USB host link.
//!
//! Two parties read it, never both at once:
//!
//! - **core-only** (no engine runs) reads it straight off the link
//!   (`with_link(|l| l.recv())`) and answers with
//!   [`UsbLinkShared::send_update`];
//! - **while the engine runs**, the engine's transport owns the link's
//!   receive side. It hands every channel-3 message to the hook the core
//!   installed ([`set_update_hook`]), and calls the hook once more, with
//!   `None`, on every pass over the link, so a hook that queued answers it
//!   could not send yet (the send ring full, the frame buffer busy) sends
//!   them as room appears. The hook is core code; it answers through
//!   [`UsbLinkShared::send_update`] too.
//!
//! The hook runs in task context (the server loop's), never in an
//! interrupt, so it needs no RAM placement; it must not block.
//!
//! **Sending.** An answer that fits the send ring ([`RING_MAX`] bytes: `R`,
//! `N`, `M`, a login step) is queued there. A larger one — a read-back `D`,
//! one 4 KiB chunk — goes as the link's one **external** message out of the
//! static frame buffer (feature `server`), when no reply holds it: the
//! board's send ring is cut to ~2.5 KiB for heap, and no update message is
//! worth growing it. Either way a busy link says [`UpdateSend::Later`] and
//! the caller keeps the answer.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The hook the engine's transport hands channel 3 to (`0`: none).
static UPDATE_HOOK: AtomicUsize = AtomicUsize::new(0);

/// When the hook last got a channel-3 message (milliseconds since boot,
/// wrapping; `0`: never).
static LAST_MESSAGE_MS: AtomicU32 = AtomicU32::new(0);

/// How recent a channel-3 message must be for the update channel to count as
/// streaming (a host pulling a read-back, one `G` after another).
const STREAMING_WITHIN_MS: u32 = 15;

/// The largest channel-3 message queued in the send ring; anything larger
/// goes as an external message out of the frame buffer.
pub const RING_MAX: usize = 1024;

/// What [`UsbLinkShared::send_update`] did with a message.
///
/// [`UsbLinkShared::send_update`]: super::UsbLinkShared::send_update
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateSend {
    /// Queued on channel 3.
    Queued,
    /// The link has no room right now (the send ring is full, or a reply
    /// holds the frame buffer): keep it and try again.
    Later,
    /// No host has the link up: the message belongs to no session.
    NoSession,
    /// Larger than this image can send (no frame buffer without `server`,
    /// or past the link's largest message).
    TooBig,
}

/// Install the core's channel-3 hook for while the engine runs: `Some` with
/// each message, `None` once per pass to flush what it holds.
pub fn set_update_hook(hook: fn(Option<&[u8]>)) {
    UPDATE_HOOK.store(hook as usize, Ordering::Release);
}

/// The engine's transport: a channel-3 message (`Some`), or a pass over the
/// link (`None`).
pub(crate) fn dispatch_update(message: Option<&[u8]>) {
    let raw = UPDATE_HOOK.load(Ordering::Acquire);
    if raw != 0 {
        if message.is_some() {
            LAST_MESSAGE_MS.store(now_ms().max(1), Ordering::Relaxed);
        }
        // SAFETY: only `set_update_hook` stores here, and it stores a
        // `fn(Option<&[u8]>)`.
        let hook: fn(Option<&[u8]>) = unsafe { core::mem::transmute(raw) };
        hook(message);
    }
}

/// Whether a host is streaming over channel 3 right now: a message reached
/// the hook within the last few milliseconds. The transport then keeps
/// pumping the link for a while before the server loop renders its next
/// frame (`UsbLinkTransport::stream_update`), so a read-back moves several
/// sectors per frame instead of one.
pub(crate) fn streaming() -> bool {
    let last = LAST_MESSAGE_MS.load(Ordering::Relaxed);
    last != 0 && now_ms().wrapping_sub(last) <= STREAMING_WITHIN_MS
}

fn now_ms() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}
