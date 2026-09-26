//! Which host link session the board is in: a counter the link bumps each
//! time the host goes away.
//!
//! Per-link state that a host negotiates — today only the wire encoding a
//! host opted into (`ClientRequest::SetEncoding`, plan `lp-json-pack`) —
//! must not outlive the host that asked for it. A serial monitor opened after
//! Studio closed the port must see today's `M!{json}` lines, not packed
//! frames it cannot read. The transport that holds such state records the
//! epoch it was negotiated in and treats a different epoch as "a new link:
//! back to the defaults".
//!
//! Bumped by [`crate::serial::usb_connection::UsbLinkState`] on the two
//! "the host is gone" transitions it can see: the cable de-enumerates (SOF
//! stops), and the host application stops draining (the only sign on a
//! USB-Serial-JTAG link that a port was closed — SOF keeps arriving while
//! the cable is in). A board reset starts at epoch 0 with fresh defaults, so
//! it needs no bump. A UART link (the classic) has no such signal; there a
//! host opening the port resets the board, which is the same thing.
//!
//! A bare relaxed atomic, like [`crate::serial::link_counters`], so it is
//! safe to bump from the io task on any executor.

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

static LINK_EPOCH: AtomicU32 = AtomicU32::new(0);

/// The current link epoch.
pub fn current() -> u32 {
    LINK_EPOCH.load(Relaxed)
}

/// The host went away: whatever it negotiated no longer applies.
pub fn bump() {
    LINK_EPOCH.fetch_add(1, Relaxed);
}
