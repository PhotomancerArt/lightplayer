//! Host/test backend: region in an ordinary owned buffer.

use crate::backend::RecoveryBackend;
use crate::recovery::{BootAssessment, Recovery};
use crate::recovery_region::RecoveryRegion;
use crate::reset_cause::ResetCause;

/// A [`RecoveryBackend`] over an owned region. Used by unit tests and by
/// host runtimes (fw-host, fw-browser) where "persistence" correctly means
/// "for the lifetime of the process".
///
/// Reboot simulation: [`Recovery::into_backend`] hands the region back with
/// its bytes intact — re-`init` with a chosen [`ResetCause`] and the new
/// run sees exactly what a soft reset would have preserved. The emulator
/// test harness mirrors this pattern with real guest RAM.
pub struct InMemoryBackend {
    region: RecoveryRegion,
    reset_requested: bool,
}

impl InMemoryBackend {
    /// Fresh backend with garbage-equivalent (zeroed) region contents, as
    /// after a power-on.
    pub fn new() -> Self {
        Self {
            region: RecoveryRegion::ZEROED,
            reset_requested: false,
        }
    }

    /// Whether `request_reset` was called since the last `clear_reset_request`.
    pub fn reset_requested(&self) -> bool {
        self.reset_requested
    }

    pub fn clear_reset_request(&mut self) {
        self.reset_requested = false;
    }

    /// Simulate a reboot: consume the running `Recovery`, keep the region
    /// bytes, and boot again with `cause`.
    pub fn reboot(
        recovery: Recovery<InMemoryBackend>,
        cause: ResetCause,
    ) -> (Recovery<InMemoryBackend>, BootAssessment) {
        let mut backend = recovery.into_backend();
        backend.reset_requested = false;
        Recovery::init(backend, cause)
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryBackend for InMemoryBackend {
    fn region(&mut self) -> &mut RecoveryRegion {
        &mut self.region
    }

    fn request_reset(&mut self) {
        self.reset_requested = true;
    }
}
