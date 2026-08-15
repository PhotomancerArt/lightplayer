//! The page-wide "a user open is in flight" signal.
//!
//! Clicking a project or an example is the one interaction that has to feel
//! immediate, so while its open runs the background preview work stops
//! *starting*: no new pool boots, no new lease deploys, no new
//! hover-to-play leases. The engine's fetch/compile/WebGPU budget belongs
//! to the click for as long as it lasts.
//!
//! The pause is a deferral, never a cancellation. Work already in flight
//! finishes (a poster capture mid-flight keeps its slot), nothing is
//! queued behind the signal, and every deferred decision is simply
//! reconsidered on the next scheduler tick once the open settles.
//!
//! This is a *signal* rather than a plumbed handle because its producer and
//! its consumers never meet: [`crate::StudioController`] opens projects
//! long before the page lazily constructs its `PreviewHost`, and the
//! gallery's hover tasks belong to neither. Everything here runs on the
//! browser's single thread, so a thread-local counter IS the shared state;
//! native builds get one per test thread, which keeps unit tests
//! independent.
//!
//! Counted rather than boolean: opens overlap — a second click supersedes
//! the first — and the pause must last until the last of them settles.

use core::cell::Cell;

thread_local! {
    static USER_OPENS_IN_FLIGHT: Cell<u32> = const { Cell::new(0) };
}

/// Whether at least one user-initiated open is in flight right now.
///
/// Consulted by the preview host's scheduler (boots, lease starts) and by
/// the gallery's hover-to-play leases.
pub fn user_open_in_flight() -> bool {
    USER_OPENS_IN_FLIGHT.with(|count| count.get() > 0)
}

/// Mark a user-initiated open as in flight until the returned guard drops.
///
/// RAII on purpose: an open that fails, is superseded, or has its future
/// dropped mid-await must not leave the page's background work paused for
/// the rest of its life.
#[must_use = "the signal clears as soon as the guard drops"]
pub fn begin_user_open() -> UserOpenGuard {
    USER_OPENS_IN_FLIGHT.with(|count| count.set(count.get().saturating_add(1)));
    UserOpenGuard { _private: () }
}

/// Holds the open-in-flight signal for one open (see [`begin_user_open`]).
pub struct UserOpenGuard {
    _private: (),
}

impl Drop for UserOpenGuard {
    fn drop(&mut self) {
        USER_OPENS_IN_FLIGHT.with(|count| count.set(count.get().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_with_no_open_running_never_holds_the_signal() {
        assert!(!user_open_in_flight());
    }

    #[test]
    fn an_open_holds_the_signal_and_clears_it_on_settle() {
        {
            let _open = begin_user_open();
            assert!(user_open_in_flight());
        }
        assert!(!user_open_in_flight());
    }

    #[test]
    fn overlapping_opens_hold_the_signal_until_the_last_one_settles() {
        // The supersede case: a second click lands while the first open is
        // still unwinding. Releasing the first must not resume background
        // work under the second.
        let first = begin_user_open();
        let second = begin_user_open();
        drop(first);
        assert!(user_open_in_flight());
        drop(second);
        assert!(!user_open_in_flight());
    }
}
