//! The one UART host link, as the link task and the server transport share
//! it.
//!
//! Both run on the thread executor (the main task and a task it spawned), so
//! a plain [`RefCell`] is the lock, exactly as on the C6
//! (`crate::usb_link::UsbLinkShared`): every borrow is taken inside a
//! synchronous call and dropped before any `.await`, so the two never
//! overlap. The classic's I/O task, which preempts both from its interrupt
//! executor, never touches this — it moves bytes through
//! [`super::uart_link_pipes`] (ruling DD20 of plan `classic-uart-on-lp-link`).
//! The one wake the transport gives the link task is the send doorbell, a
//! [`Signal`].

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lp_link::{Link, LinkState, SelectiveRepeat};

use super::uart_link_config::uart_board_link_config;

/// The link, and the doorbell that wakes its task.
pub struct UartLinkShared {
    link: RefCell<Link<SelectiveRepeat>>,
    doorbell: Signal<CriticalSectionRawMutex, ()>,
}

impl UartLinkShared {
    /// A new link for this boot, on the board's configuration
    /// ([`uart_board_link_config`]), leaked for the task and the transport to
    /// share. `nonce` must be random per boot (the chip's RNG): it is how the
    /// host learns the board restarted.
    pub fn leak(nonce: u32) -> &'static Self {
        Box::leak(Box::new(Self {
            link: RefCell::new(Link::new(uart_board_link_config(), nonce)),
            doorbell: Signal::new(),
        }))
    }

    /// Run `f` on the link. Never call this from inside another `with_link`,
    /// never hold what `f` returns across an `.await`, and never call it from
    /// the I/O task (see the module docs).
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

    /// Something was queued: wake the link task to transmit it now rather
    /// than at its next timer.
    pub fn ring(&self) {
        self.doorbell.signal(());
    }

    /// The link task's side of [`Self::ring`].
    pub(crate) async fn doorbell(&self) {
        self.doorbell.wait().await;
    }

    /// The doorbell itself, for the log ring to ring when a record lands
    /// ([`crate::log_ring_logger::ring_on_record`]), mirroring the C6/S3's
    /// `UsbLinkShared::doorbell_signal`.
    pub(crate) fn doorbell_signal(&'static self) -> &'static Signal<CriticalSectionRawMutex, ()> {
        &self.doorbell
    }
}
