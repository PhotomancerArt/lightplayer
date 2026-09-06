//! What an output provider did to a port's smoothing, relative to what was
//! authored — the board-side, scale-dependent downgrade made visible.

/// The smoothing a port actually runs with, when it differs from what the
/// author asked for.
///
/// A provider on a memory-limited board may open pipelines with frame
/// interpolation or temporal dithering OFF once the board's total open
/// lamps exceed the measured limits its manifest carries (see
/// `lpc_hardware::HwSoftLimits`). That is a deliberate quality trade, and
/// a silent one would be a lie on the wall: the output looks a little
/// rougher and nothing says why. So the provider reports it here and the
/// engine turns it into the output node's `Warn` status.
///
/// `None` from [`super::OutputProvider::port_smoothing`] means nothing was
/// reduced — including every provider that never reduces (the trait
/// default), and a port whose author already turned the feature off (the
/// provider never turns a feature ON).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPortSmoothing {
    /// Total lamps open across the whole board at the last re-tier — the
    /// number the limits were compared against.
    pub total_lamps: u32,
    /// `Some(limit)` when interpolation was authored on but opened OFF
    /// because `total_lamps > limit`.
    pub interpolation_off_above: Option<u32>,
    /// `Some(limit)` when dithering was authored on but opened OFF because
    /// `total_lamps > limit`.
    pub dithering_off_above: Option<u32>,
}

impl OutputPortSmoothing {
    /// Nothing reduced — the value a provider folds away into `None`.
    pub fn is_unreduced(&self) -> bool {
        self.interpolation_off_above.is_none() && self.dithering_off_above.is_none()
    }
}
