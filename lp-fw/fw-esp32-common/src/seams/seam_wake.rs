//! The wake's firmware half (`lp_seam::wake`): the pending word, and what
//! the handler does with it.
//!
//! An emulator with something for an idle guest sets bits in [`PENDING`],
//! then raises the wake line (`FROM_CPU_INTR3`, priority 1). The chip
//! crate's handler clears the line first, then calls [`take_pending`], which
//! swaps the word to zero and, when it was non-zero, wakes every consumer.
//! A raise that lands between the clear and the swap is seen by the next
//! take, never lost.
//!
//! **The consumers are the network seam's two waiters**, and only tasks on
//! `lp-net` register them (G0 rule (a): whatever a capability seam wakes runs
//! on the IO thread, never the main or render executor):
//!
//! - [`NET_FRAMES`]: the seam frame device's waker (embassy-net's runner):
//!   a frame arrived, or the link changed;
//! - [`NET_EVENTS`]: the seam station's waker (the station task): a station
//!   event (`lp_seam::net::EVENT_*`) is waiting.
//!
//! Any bit wakes both: a woken consumer that finds nothing goes back to
//! waiting, and an `associated` event must reach the frame device too (its
//! link went up). [`NET_FRAMES_BIT`] and [`NET_EVENTS_BIT`] are the bits the
//! emulator is asked to set, so a trace can tell the two apart.
//!
//! On silicon the line is never bound (the chip binds it only when the
//! network seam is engaged) and the word stays zero: its whole cost is the
//! word and the two waker cells in `.bss`.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::waitqueue::AtomicWaker;

/// The wake pending word: the table's `pending` field is its address.
pub static PENDING: AtomicU32 = AtomicU32::new(0);

/// The network seam's frame endpoint: a frame is waiting, or the link moved.
pub const NET_FRAMES_BIT: u32 = 1 << 0;
/// The network seam's station endpoint: a station event is waiting.
pub const NET_EVENTS_BIT: u32 = 1 << 1;

/// The seam frame device's waiter (embassy-net's runner, on `lp-net`).
pub static NET_FRAMES: AtomicWaker = AtomicWaker::new();
/// The seam station's waiter (the station task, on `lp-net`).
pub static NET_EVENTS: AtomicWaker = AtomicWaker::new();

/// The handler's body, after it cleared the line: swap the word to zero
/// and wake every consumer when any bit was set. `inline(always)` so it is
/// emitted inside the chip's `#[ram]` handler, never as a flash function of
/// its own (plain `#[inline]` is ignored at `opt-level = "z"`).
#[inline(always)]
pub fn take_pending() {
    if PENDING.swap(0, Ordering::AcqRel) != 0 {
        NET_FRAMES.wake();
        NET_EVENTS.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicUsize;
    use core::task::{RawWaker, RawWakerVTable, Waker};

    #[test]
    fn a_take_clears_the_word_and_wakes_both_consumers_once_per_set_word() {
        NET_FRAMES.register(&counting_waker());
        NET_EVENTS.register(&counting_waker());
        let before = WAKES.load(Ordering::SeqCst);

        // Nothing pending: nobody is woken.
        take_pending();
        assert_eq!(WAKES.load(Ordering::SeqCst), before);

        // Any bit wakes both.
        PENDING.fetch_or(NET_EVENTS_BIT, Ordering::SeqCst);
        take_pending();
        assert_eq!(PENDING.load(Ordering::SeqCst), 0);
        assert_eq!(WAKES.load(Ordering::SeqCst), before + 2);

        // The word is read once: a second take finds it zero.
        take_pending();
        assert_eq!(WAKES.load(Ordering::SeqCst), before + 2);
    }

    #[test]
    fn the_two_endpoints_have_distinct_bits() {
        assert_ne!(NET_FRAMES_BIT, NET_EVENTS_BIT);
        assert_eq!(NET_FRAMES_BIT & NET_EVENTS_BIT, 0);
    }

    static WAKES: AtomicUsize = AtomicUsize::new(0);

    fn counting_waker() -> Waker {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(core::ptr::null(), &VTABLE),
            |_| {
                WAKES.fetch_add(1, Ordering::SeqCst);
            },
            |_| {
                WAKES.fetch_add(1, Ordering::SeqCst);
            },
            |_| {},
        );
        // SAFETY: every vtable entry ignores its (null) data pointer.
        unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
    }
}
