use crate::Revision;

// Production: a process-wide atomic, advanced by the single frame-orchestration
// owner. Test builds: a per-thread cell, so parallel tests that set/advance the
// ambient revision are isolated and don't race through shared global state
// (libtest runs tests across many threads, one per test).
//
// `cfg(test)` only covers this crate's own unit tests — it is NOT set when a
// downstream crate's test binary (e.g. lpc-engine's) compiles lpc-model as a
// dependency, so those parallel tests would race on the shared atomic. The
// `test-support` feature exists for exactly that gap: downstream crates whose
// tests set/advance the ambient revision enable it from `[dev-dependencies]`
// only, which scopes the thread-local to their test builds without ever
// reaching production or firmware (`no_std`) builds, which keep the atomic.
#[cfg(not(any(test, feature = "test-support")))]
static CURRENT_REVISION: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

#[cfg(any(test, feature = "test-support"))]
std::thread_local! {
    static CURRENT_REVISION: core::cell::Cell<i32> = core::cell::Cell::new(0);
}

/// Current ambient synchronized-state revision.
///
/// Slot values, slot containers, shape registries, and other synchronized state
/// read this revision when stamping mutations. Runtime orchestration owns when
/// the ambient revision advances; data containers should normally read it, not
/// advance it themselves.
pub fn current_revision() -> Revision {
    #[cfg(not(any(test, feature = "test-support")))]
    let raw = CURRENT_REVISION.load(core::sync::atomic::Ordering::Relaxed);
    #[cfg(any(test, feature = "test-support"))]
    let raw = CURRENT_REVISION.with(core::cell::Cell::get);
    Revision::new(raw as i64)
}

/// Set the ambient synchronized-state revision.
///
/// This is primarily for runtime frame orchestration, loading, replay, and
/// focused tests. Ordinary slot data mutation should stamp the current revision
/// rather than setting it.
pub fn set_current_revision(revision: Revision) {
    let raw = revision.as_i64() as i32;
    #[cfg(not(any(test, feature = "test-support")))]
    CURRENT_REVISION.store(raw, core::sync::atomic::Ordering::Relaxed);
    #[cfg(any(test, feature = "test-support"))]
    CURRENT_REVISION.with(|cell| cell.set(raw));
}

/// Advance the ambient synchronized-state revision and return the new value.
pub fn advance_revision() -> Revision {
    #[cfg(not(any(test, feature = "test-support")))]
    let next = CURRENT_REVISION.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1;
    #[cfg(any(test, feature = "test-support"))]
    let next = CURRENT_REVISION.with(|cell| {
        let next = cell.get() + 1;
        cell.set(next);
        next
    });
    Revision::new(next as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_can_be_set_and_advanced() {
        set_current_revision(Revision::new(10));

        assert_eq!(current_revision(), Revision::new(10));
        assert_eq!(advance_revision(), Revision::new(11));
        assert_eq!(current_revision(), Revision::new(11));
    }
}
