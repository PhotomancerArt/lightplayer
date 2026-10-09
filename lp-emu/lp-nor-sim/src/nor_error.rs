//! What a flash operation can fail with.

/// A failed flash operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NorError {
    /// Power was cut: the operation that hit the cut was torn, and every
    /// operation since (reads included) fails until `power_cycle`.
    PowerLost,
    /// The address range is outside the part.
    OutOfBounds,
    /// The read watchdog fired: more reads since the last power cycle than
    /// the configured budget, the sign of a store looping on bad flash.
    Watchdog,
}
