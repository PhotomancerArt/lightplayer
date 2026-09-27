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
const LOG_DATAGRAMS: usize = 8;

/// Replies the link may hold queued (entries, not bytes: the byte budget is
/// [`MAX_MESSAGE`]).
const SEND_QUEUE: usize = 16;

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
    /// largest free block). The floor is one largest reply in flight: the send
    /// ring is exactly [`MAX_MESSAGE`], so while a 16 KiB frame is unacknowledged
    /// the next reply waits for room (the transport waits it out), as the
    /// one-frame-in-flight `M!` path did. The receive side is lazy (it grows
    /// with traffic), capped at one largest request.
    pub fn config() -> LinkConfig {
        let mut cfg = LinkConfig::usb();
        cfg.max_message = MAX_MESSAGE;
        cfg.send_budget = MAX_MESSAGE;
        // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
        cfg.rx_budget = MAX_MESSAGE + 64;
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

    /// Queue one proto-channel message without waiting, and wake the task;
    /// `false` if the link refused it (no session, or no room). For a
    /// harness; the server's transport accounts for its sends itself.
    pub fn try_send_proto(&self, payload: &[u8]) -> bool {
        let queued = self.with_link(|link| {
            link.state() == LinkState::Established && link.send(lp_link::CH_PROTO, payload).is_ok()
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
