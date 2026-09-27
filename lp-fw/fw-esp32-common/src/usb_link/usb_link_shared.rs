//! The one USB host link, as the link task and the server transport share it.
//!
//! Both run on the chip's one thread executor (the main task and a task it
//! spawned), so a plain [`RefCell`] is the lock: every borrow is taken inside
//! a synchronous call and dropped before any `.await`, so the two never
//! overlap. That is deliberate — a critical-section mutex here would mask
//! interrupts for as long as `Link::send` copies a 16 KB reply, and the RMT
//! refill (the LEDs) cannot wait that long. The one cross-task wake is the
//! send doorbell, a [`Signal`].

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lp_link::{Link, LinkConfig, LinkState, SelectiveRepeat};

/// The largest message the board sends or takes: the static frame buffer's
/// size, which bounds every reply it can serialize (the 16 KiB `ProjectRead`
/// frame budget plus its serial margin), and the wire's budget for a request.
#[cfg(feature = "server")]
pub const MAX_MESSAGE: usize = crate::serial::server_msg::SERVER_MSG_JSON_BUFFER_SIZE;
#[cfg(not(feature = "server"))]
pub const MAX_MESSAGE: usize = lp_link::MAX_MESSAGE;

/// Log records the link may hold queued at once. Each slot is a whole frame
/// payload (256 B) allocated for the link's life; the log ring (4 KB) holds
/// the rest, and the link task moves at most
/// [`super::usb_link_task`]'s per-pass count into it.
const LOG_DATAGRAMS: usize = 4;

/// Messages the send ring may hold queued. Replies go as external messages
/// (one at a time, from the frame buffer); the ring carries only the small
/// ones built elsewhere (a dropped reply's error notice).
const SEND_QUEUE: usize = 4;

/// The send ring: the small messages above, plus the transmit window's bytes,
/// which count against the same budget (8 frames x 256 B = 2 KiB while a reply is
/// going out).
const SEND_BUDGET: usize = 2560;

/// A reassembly buffer above this is released once its request is delivered
/// (an upload's chunk grows one to ~6 KB; it must not stay).
const KEEP_REASSEMBLY: usize = 1024;

/// The link, and the doorbell that wakes its task.
pub struct UsbLinkShared {
    link: RefCell<Link<SelectiveRepeat>>,
    doorbell: Signal<CriticalSectionRawMutex, ()>,
}

impl UsbLinkShared {
    /// The board's link configuration: lp-link's USB preset, its buffers cut
    /// to what the board needs.
    ///
    /// Every buffer is allocated in `Link::new` for the link's life, and the
    /// preset's (24 KiB send ring, 32 log slots) would hold ~39 KB of a heap a
    /// loaded project leaves tight (a `ProjectRead` is refused under a 32 KB
    /// largest free block). Replies are not copied into the link at all: the
    /// transport serializes each into the static frame buffer and queues it
    /// as an external message, whose fragments the link cuts from there into
    /// its transmit window (`Link::send_external`). So the send ring is small
    /// ([`SEND_BUDGET`]), and the receive side is lazy (it grows with traffic,
    /// is capped at one largest request, and gives a large reassembly buffer
    /// back once its request is delivered).
    pub fn config() -> LinkConfig {
        let mut cfg = LinkConfig::usb();
        cfg.max_message = MAX_MESSAGE;
        cfg.send_budget = SEND_BUDGET;
        // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
        cfg.rx_budget = MAX_MESSAGE + 64;
        cfg.keep_reassembly = KEEP_REASSEMBLY;
        cfg.send_queue = SEND_QUEUE;
        cfg.datagram_queue = LOG_DATAGRAMS;
        cfg
    }

    /// A new link for this boot, leaked for the task and the transport to
    /// share. `nonce` must be random per boot (the chip's RNG): it is how the
    /// host learns the board restarted.
    pub fn leak(nonce: u32) -> &'static Self {
        Box::leak(Box::new(Self {
            link: RefCell::new(Link::new(Self::config(), nonce)),
            doorbell: Signal::new(),
        }))
    }

    /// Run `f` on the link. Never call this from inside another `with_link`,
    /// and never hold what `f` returns across an `.await` (see the module
    /// docs).
    pub fn with_link<R>(&self, f: impl FnOnce(&mut Link<SelectiveRepeat>) -> R) -> R {
        f(&mut self.link.borrow_mut())
    }

    /// Whether a host has the link up right now.
    pub fn is_established(&self) -> bool {
        self.with_link(|link| link.state() == LinkState::Established)
    }

    /// The link is still reading a reply out of the static frame buffer:
    /// nothing may serialize into it yet.
    pub fn frame_buf_in_use(&self) -> bool {
        self.with_link(|link| link.external_in_flight())
    }

    /// Queue the `len` bytes the caller serialized into the static frame
    /// buffer as one proto-channel message, without waiting, and wake the
    /// task; `false` if the link refused it (no session, or one still in
    /// flight). For a harness; the server's transport accounts for its sends
    /// itself. Check [`Self::frame_buf_in_use`] before serializing.
    pub fn try_send_frame_buf(&self, len: usize) -> bool {
        let queued = self.with_link(|link| {
            link.state() == LinkState::Established
                && link.send_external(lp_link::CH_PROTO, len).is_ok()
        });
        if queued {
            self.ring();
        }
        queued
    }

    /// Something was queued: wake the link task to transmit it now rather
    /// than at its next timer.
    pub fn ring(&self) {
        self.doorbell.signal(());
    }

    /// The link task's side of [`Self::ring`].
    pub(crate) async fn doorbell(&self) {
        self.doorbell.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The board's cut holds together, carries the largest reply, and costs
    /// well under the preset. (`--nocapture` prints the figures.)
    #[test]
    fn the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset() {
        let cfg = UsbLinkShared::config();
        assert_eq!(cfg.validate(), Ok(()));
        assert!(cfg.max_message >= lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES);
        assert!(
            cfg.send_budget >= cfg.tx_window as usize * cfg.max_payload as usize + 512,
            "the ring holds a full window plus a notice"
        );
        let board = Link::<SelectiveRepeat>::new(cfg.clone(), 1).ram_bytes();
        let preset = Link::<SelectiveRepeat>::new(LinkConfig::usb(), 1).ram_bytes();
        let bound = Link::<SelectiveRepeat>::ram_bound(&cfg);
        extern crate std;
        std::println!(
            "usb link RAM at rest: board {board} B, preset {preset} B; board bound {bound} B"
        );
        assert!(
            board + 8 * 1024 < preset,
            "board {board} B vs preset {preset} B"
        );
    }
}
