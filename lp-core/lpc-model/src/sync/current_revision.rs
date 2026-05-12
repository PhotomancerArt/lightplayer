use crate::Revision;
use core::sync::atomic::{AtomicI32, Ordering};

static CURRENT_REVISION: AtomicI32 = AtomicI32::new(0);

/// Current ambient synchronized-state revision.
///
/// Slot values, slot containers, shape registries, and other synchronized state
/// read this revision when stamping mutations. Runtime orchestration owns when
/// the ambient revision advances; data containers should normally read it, not
/// advance it themselves.
pub fn current_revision() -> Revision {
    Revision::new(CURRENT_REVISION.load(Ordering::Relaxed) as i64)
}

/// Set the ambient synchronized-state revision.
///
/// This is primarily for runtime frame orchestration, loading, replay, and
/// focused tests. Ordinary slot data mutation should stamp the current revision
/// rather than setting it.
pub fn set_current_revision(revision: Revision) {
    CURRENT_REVISION.store(revision.as_i64() as i32, Ordering::Relaxed);
}

/// Advance the ambient synchronized-state revision and return the new value.
pub fn advance_revision() -> Revision {
    Revision::new((CURRENT_REVISION.fetch_add(1, Ordering::Relaxed) + 1) as i64)
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
