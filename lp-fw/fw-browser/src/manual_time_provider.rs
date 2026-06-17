//! Manual browser time source.
//!
//! Browser firmware tests and Studio previews need deterministic ticks, so time
//! advances only when the embedding code sends a `tick` envelope.

use std::cell::RefCell;
use std::rc::Rc;

use lpc_shared::time::TimeProvider;

/// Shared deterministic millisecond clock for one browser firmware runtime.
#[derive(Clone)]
pub(crate) struct ManualTimeProvider {
    now_ms: Rc<RefCell<u64>>,
}

impl ManualTimeProvider {
    /// Create a manual clock starting at zero.
    pub(crate) fn new() -> Self {
        Self {
            now_ms: Rc::new(RefCell::new(0)),
        }
    }

    /// Move the clock forward by the requested tick amount.
    pub(crate) fn advance(&self, delta_ms: u32) {
        let mut now = self.now_ms.borrow_mut();
        *now = now.saturating_add(u64::from(delta_ms));
    }
}

impl TimeProvider for ManualTimeProvider {
    fn now_ms(&self) -> u64 {
        *self.now_ms.borrow()
    }
}
