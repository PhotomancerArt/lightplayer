use super::FrameId;
use core::sync::atomic::{AtomicI32, Ordering};

static CURRENT_STATE_VERSION: AtomicI32 = AtomicI32::new(0);

/// Current observable state version.
///
/// Slot values, shape registries, and other synchronized state use this version
/// to mark when data last changed. Runtime orchestration advances or sets the
/// version once per observable update epoch; leaf containers only read it.
pub fn current_state_version() -> FrameId {
    FrameId::new(CURRENT_STATE_VERSION.load(Ordering::Relaxed) as i64)
}

/// Set the ambient observable state version.
///
/// This is primarily for runtime frame orchestration, loading, replay, and
/// focused tests. Ordinary slot data mutation should stamp the current version
/// rather than setting it.
pub fn set_current_state_version(version: FrameId) {
    CURRENT_STATE_VERSION.store(version.as_i64() as i32, Ordering::Relaxed);
}

/// Advance the ambient observable state version and return the new value.
pub fn advance_state_version() -> FrameId {
    FrameId::new((CURRENT_STATE_VERSION.fetch_add(1, Ordering::Relaxed) + 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_version_can_be_set_and_advanced() {
        set_current_state_version(FrameId::new(10));

        assert_eq!(current_state_version(), FrameId::new(10));
        assert_eq!(advance_state_version(), FrameId::new(11));
        assert_eq!(current_state_version(), FrameId::new(11));
    }
}
