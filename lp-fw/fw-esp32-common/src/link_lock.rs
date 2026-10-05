//! The lock hook both host links share: how a chip keeps a link's two users
//! apart when they may run on different threads.
//!
//! A board's one lp-link `Link` has two users — the link task, which feeds it
//! bytes and cuts its frames, and the server transport, which takes requests
//! off it and queues replies onto it. On one executor they never run at once
//! and need no lock; once a chip gives the link task a thread of its own (the
//! C6's and S3's `io_thread`, the classic's), the link thread can preempt the
//! main thread inside a borrow, and the chip injects a [`LinkLock`] that every
//! borrow runs inside. `fw-esp32-common` names no esp-hal thread or lock API,
//! so the chip supplies the function; the shared links
//! (`crate::usb_link::UsbLinkShared`, `crate::uart_link::UartLinkShared`) only
//! call it.

/// Runs its argument with the link's two users kept apart, and must run it
/// exactly once. The default ([`no_lock`]) runs it directly: both users on one
/// executor.
pub type LinkLock = fn(&mut dyn FnMut());

/// The lock for both users on one executor: none.
pub(crate) fn no_lock(f: &mut dyn FnMut()) {
    f()
}
