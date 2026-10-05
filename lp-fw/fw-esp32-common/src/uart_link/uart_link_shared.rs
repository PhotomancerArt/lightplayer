//! The one UART host link, as the link task and the server transport share
//! it.
//!
//! Every use of the link goes through [`UartLinkShared::with_link`]: a
//! synchronous call whose [`RefCell`] borrow is dropped before any `.await`.
//! The classic's I/O task, which preempts both from its interrupt executor
//! (swi2, Priority2), never touches this — it moves bytes through
//! [`super::uart_link_pipes`] (ruling DD20 of plan `classic-uart-on-lp-link`).
//! The one cross-task wake is the doorbell, a [`Signal`] — rung by the
//! transport when it queues a reply and by the log ring when a record lands.
//!
//! **Two arrangements, one lock hook** — the C6/S3's
//! (`crate::usb_link::usb_link_shared`), for this link:
//!
//! - *One executor* (`fw-esp32v3` without `io-thread`): the link task is a
//!   task the main task spawned, so the two users never run at once and the
//!   `RefCell` alone keeps them apart. [`UartLinkShared::leak`] injects no
//!   lock.
//! - *Two threads* (`fw-esp32v3`'s `io_thread`: the link task on a
//!   priority-1 esp-rtos thread pinned to core 0): the link thread can
//!   preempt the main thread inside a borrow, so the chip injects a
//!   [`LinkLock`] ([`UartLinkShared::leak_locked`]) that every `with_link`
//!   runs inside. The `RefCell` stays, as the overlap detector: anything the
//!   lock let through would panic rather than alias a `&mut`.
//!
//! **The short-closure rule.** With the classic's lock (a priority-limited
//! mutex at Priority1), every `with_link` closure masks everything at
//! priority 1 on core 0 for as long as it runs: esp-rtos's context-switch
//! software interrupt and its timer tick, the I/O task's 1 ms pacer, and
//! UART0's interrupt. It never masks the I/O task's own executor (Priority2),
//! the RMT refill ISR (level 3, on core 1 or its core-0 fallback) or the
//! APP core's wire-pusher doorbell (core 1: masks are per core). The bound
//! that matters is the 128-byte RX FIFO, which fills in ~1.4 ms at 921,600
//! baud while the pacer that drains it is held off. Today's closures are
//! microseconds against that: an `on_bytes` of at most 128 B, one frame of
//! at most 533 B cut into the task's own buffer, an event pop, a reply queued
//! by `send_external` without a copy (the link reads it from the static frame
//! buffer later, a frame at a time), a drop notice of at most 192 B, and a
//! log pump of at most two records. Keep it that way: no
//! serialization, no logging and no loop over the pipes inside one closure.

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lp_link::{Link, LinkState, SelectiveRepeat};

pub use crate::link_lock::LinkLock;
use crate::link_lock::no_lock;

use super::uart_link_config::uart_board_link_config;

/// The link, the lock its users share, and the doorbell that wakes its task.
pub struct UartLinkShared {
    link: RefCell<Link<SelectiveRepeat>>,
    doorbell: Signal<CriticalSectionRawMutex, ()>,
    lock: LinkLock,
}

// SAFETY: `link` is the only field that is not `Sync`, and it is reached only
// through `with_link`, which borrows it inside `lock`. With the default lock
// (`leak`) both users share one executor and never run at once; a chip that
// puts them on different threads must inject a lock that keeps them apart
// (`leak_locked`), and the `RefCell` turns any overlap the lock lets through
// into a panic rather than an aliased `&mut`. The doorbell is a
// critical-section `Signal`, `Sync` on its own; `lock` is a plain `fn`.
unsafe impl Sync for UartLinkShared {}

impl UartLinkShared {
    /// A new link for this boot, on the board's configuration
    /// ([`uart_board_link_config`]), leaked for the task and the transport to
    /// share. `nonce` must be random per boot (the chip's RNG): it is how the
    /// host learns the board restarted.
    pub fn leak(nonce: u32) -> &'static Self {
        Self::leak_locked(nonce, no_lock)
    }

    /// [`Self::leak`] for a chip whose link task and transport run on
    /// different threads: every [`Self::with_link`] runs inside `lock` (see
    /// the module docs).
    pub fn leak_locked(nonce: u32, lock: LinkLock) -> &'static Self {
        Box::leak(Box::new(Self {
            link: RefCell::new(Link::new(uart_board_link_config(), nonce)),
            doorbell: Signal::new(),
            lock,
        }))
    }

    /// Run `f` on the link, inside the link's lock. Keep `f` short (the
    /// module docs' rule), never call this from inside another `with_link`,
    /// never hold what `f` returns across an `.await`, and never call it from
    /// the I/O task.
    pub fn with_link<R>(&self, f: impl FnOnce(&mut Link<SelectiveRepeat>) -> R) -> R {
        let mut f = Some(f);
        let mut out = None;
        (self.lock)(&mut || {
            if let Some(f) = f.take() {
                out = Some(f(&mut self.link.borrow_mut()));
            }
        });
        match out {
            Some(out) => out,
            None => unreachable!("a LinkLock must run its argument"),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    /// Every `with_link` runs inside the injected lock, exactly once each.
    #[test]
    fn every_with_link_runs_inside_the_injected_lock() {
        static ENTERED: AtomicU32 = AtomicU32::new(0);
        fn counting_lock(f: &mut dyn FnMut()) {
            ENTERED.fetch_add(1, Ordering::Relaxed);
            f()
        }
        let shared = UartLinkShared::leak_locked(7, counting_lock);
        assert!(!shared.is_established());
        assert!(!shared.frame_buf_in_use());
        let state = shared.with_link(|link| link.state());
        assert_eq!(state, LinkState::Connecting);
        assert_eq!(ENTERED.load(Ordering::Relaxed), 3);
    }

    /// The default arrangement takes no lock at all, and still runs every
    /// closure exactly once.
    #[test]
    fn the_default_leak_runs_each_closure_once() {
        let shared = UartLinkShared::leak(9);
        let mut calls = 0;
        shared.with_link(|_| calls += 1);
        shared.with_link(|_| calls += 1);
        assert_eq!(calls, 2);
    }
}
