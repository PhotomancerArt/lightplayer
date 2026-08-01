//! The persistent recovery region in RTC fast RAM + software-reset hook.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::ram;
use lp_recovery::{RecoveryBackend, RecoveryRegion};

/// Newtype so we can promise esp-hal the region tolerates arbitrary bit
/// patterns (`Persistable` is a foreign trait; orphan rules forbid
/// implementing it for `RecoveryRegion` directly).
#[repr(transparent)]
struct PersistentRegion(RecoveryRegion);

// SAFETY: `RecoveryRegion` contains only plain integers and fixed arrays
// thereof (no references, enums, or niches) — every bit pattern is a sound
// value, and lp-recovery validates magic + CRCs before trusting contents.
unsafe impl esp_hal::Persistable for PersistentRegion {}

/// The breadcrumb region. RTC fast RAM survives software and watchdog
/// resets (NOT power loss); `persistent` skips load-time initialization so
/// the previous run's bytes are still there on soft reset. On power-up the
/// contents are undefined — `lp_recovery` validates magic + CRCs and the
/// reset reason before trusting anything.
#[ram(unstable(rtc_fast, persistent))]
static mut RECOVERY_REGION: PersistentRegion = PersistentRegion(RecoveryRegion::ZEROED);

static BACKEND_TAKEN: AtomicBool = AtomicBool::new(false);

/// [`RecoveryBackend`] over the RTC-RAM region.
pub struct Esp32RecoveryBackend {
    _private: (),
}

impl Esp32RecoveryBackend {
    /// The one and only backend instance. Panics if taken twice — the
    /// region must have a single owner (all shared access goes through the
    /// lp-recovery global's critical section).
    pub fn take() -> Self {
        assert!(
            !BACKEND_TAKEN.swap(true, Ordering::AcqRel),
            "Esp32RecoveryBackend::take() called twice"
        );
        Self { _private: () }
    }
}

impl RecoveryBackend for Esp32RecoveryBackend {
    fn region(&mut self) -> &mut RecoveryRegion {
        // SAFETY: exactly one backend exists (enforced by `take`), so this
        // is the only path to the static; `&mut self` serializes access.
        unsafe { &mut (*&raw mut RECOVERY_REGION).0 }
    }

    fn request_reset(&mut self) {
        esp_hal::system::software_reset()
    }
}
