//! Platform backend: where the region lives and how to reset the system.

use crate::recovery_region::RecoveryRegion;

/// Platform glue for the recovery system.
///
/// Implementations:
/// - ESP32: region in an RTC-fast-RAM `persistent` static; reset via
///   `esp_hal::system::software_reset()` (diverges).
/// - Emulator firmware: region in an exported static the host test harness
///   snapshots/restores; reset via a sentinel exit the harness recognizes.
/// - Host/browser/tests: [`InMemoryBackend`](crate::InMemoryBackend).
pub trait RecoveryBackend {
    /// The persistent region. Must be the SAME memory across soft resets on
    /// real hardware — that is the entire point.
    fn region(&mut self) -> &mut RecoveryRegion;

    /// Request a system reset. On real hardware this diverges; test
    /// backends record the request and return so the harness stays in
    /// control. Callers must not assume it returns OR that it diverges —
    /// firmware dead-end paths follow this call with their own
    /// platform-reset fallback.
    fn request_reset(&mut self);
}
