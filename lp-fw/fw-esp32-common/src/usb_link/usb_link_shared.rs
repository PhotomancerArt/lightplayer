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

/// The board's send budget: payload bytes the link may hold queued or
/// unacknowledged before `send` refuses.
///
/// lp-link's USB preset allows 40 KB. Every queued message is a heap copy
/// (the `M!` path wrote straight from a static buffer and held none), and a
/// loaded project leaves the C6 little heap, so the board holds one
/// `ProjectRead` frame (16.6 KB) plus room for the small messages around it;
/// the transport waits for room rather than dropping while the host drains.
pub const SEND_BUDGET: usize = 24 * 1024;

/// The link, and the doorbell that wakes its task.
pub struct UsbLinkShared {
    link: RefCell<Link<SelectiveRepeat>>,
    doorbell: Signal<CriticalSectionRawMutex, ()>,
}

impl UsbLinkShared {
    /// The board's link configuration: lp-link's USB preset with the board's
    /// send budget ([`SEND_BUDGET`]).
    pub fn config() -> LinkConfig {
        let mut cfg = LinkConfig::usb();
        cfg.send_budget = SEND_BUDGET;
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
