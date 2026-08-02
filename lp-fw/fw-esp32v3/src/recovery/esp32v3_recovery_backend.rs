//! The persistent recovery region in RTC fast RAM + the software-reset hook.
//!
//! This is the classic ESP32's implementation of `lp_recovery::RecoveryBackend`.
//! The region format, its validation, and the torn-write discipline all live in
//! `lp-recovery` and are chip-agnostic; the only chip knowledge here is *where*
//! the bytes live and *how* to reset.
//!
//! ## What survives on this chip, and what does not
//!
//! RTC fast RAM is 8 KiB at D-bus `0x3FF8_0000` (I-bus alias `0x400C_0000`),
//! and esp-hal's `ld/esp32/memory.x` claims the whole segment. It retains its
//! contents across a **software reset**, a **watchdog reset** and deep sleep.
//!
//! It does **not** retain them across a power-on reset or an EN-pin (CHIP_PU)
//! reset — and on this board EN is what espflash toggles through the CH340K's
//! DTR/RTS lines. So the flash-and-watch cycle always starts with an empty
//! ledger, and the first boot after a reflash correctly reports "nothing to
//! report" rather than a stale crash from before the flash. `lp_recovery`
//! validates magic + CRCs and the reset reason before trusting anything, so
//! undefined power-up contents cannot be mistaken for a record.
//!
//! ⚠️ That retention boundary is also the one way this instrument can come back
//! empty on a fault it *did* catch: if the RMT fault turns out to reset the
//! chip through a path that cycles the RTC domain rather than through the
//! digital-core software reset, the record is gone with it. The boot report
//! prints the mapped reset cause on every boot precisely so that case is
//! distinguishable — a `power-on` cause after a crash that was not a replug is
//! the tell.
//!
//! On this chip RTC fast RAM is reachable from PRO_CPU only. The firmware runs
//! its app on PRO_CPU and never starts APP_CPU, so that is not a constraint
//! today; it would become one the moment anything is scheduled on core 1.

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

/// The breadcrumb region. `persistent` skips load-time initialization so the
/// previous run's bytes are still there on soft reset.
///
/// `RecoveryRegion` is budgeted at <= `lp_recovery::REGION_MAX_SIZE` (1 KiB),
/// which fits the classic's 8 KiB of RTC fast RAM with room to spare. Note this
/// costs nothing from the 192 KB `dram_seg` that `.data`/`.bss`/`.stack` fight
/// over on this chip — RTC fast RAM is a separate segment, which is a rare
/// piece of good news for a build whose stack headroom is the binding
/// constraint.
#[ram(unstable(rtc_fast, persistent))]
static mut RECOVERY_REGION: PersistentRegion = PersistentRegion(RecoveryRegion::ZEROED);

static BACKEND_TAKEN: AtomicBool = AtomicBool::new(false);

/// [`RecoveryBackend`] over the RTC-RAM region.
pub struct Esp32V3RecoveryBackend {
    _private: (),
}

impl Esp32V3RecoveryBackend {
    /// The one and only backend instance. Panics if taken twice — the region
    /// must have a single owner (all shared access goes through the
    /// lp-recovery global's critical section).
    pub fn take() -> Self {
        assert!(
            !BACKEND_TAKEN.swap(true, Ordering::AcqRel),
            "Esp32V3RecoveryBackend::take() called twice"
        );
        Self { _private: () }
    }
}

impl RecoveryBackend for Esp32V3RecoveryBackend {
    fn region(&mut self) -> &mut RecoveryRegion {
        // SAFETY: exactly one backend exists (enforced by `take`), so this is
        // the only path to the static; `&mut self` serializes access.
        unsafe { &mut (*&raw mut RECOVERY_REGION).0 }
    }

    fn request_reset(&mut self) {
        esp_hal::system::software_reset()
    }
}
