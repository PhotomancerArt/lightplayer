//! Wall-clock time, injected.

/// The service's clock.
///
/// Timestamps are f64 epoch seconds, matching `lpc-history`'s convention so
/// a server-recorded time and a client-recorded one are the same kind of
/// number. The domain never reads a clock itself (AGENTS.md sans-IO), which
/// is also what makes every timestamp in a test deterministic.
pub trait Clock {
    /// The current time, f64 epoch seconds.
    fn now(&self) -> f64;
}
