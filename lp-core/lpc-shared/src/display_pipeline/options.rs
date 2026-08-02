//! Display pipeline options.

/// Color and temporal-processing options for [`super::DisplayPipeline`].
///
/// These options belong with the display pipeline rather than `lpc-hardware`:
/// hardware outputs receive already-rendered bytes, while the pipeline decides
/// how 16-bit engine samples become those bytes.
#[derive(Debug, Clone)]
pub struct DisplayPipelineOptions {
    /// RGB white point balance
    pub white_point: [f32; 3],
    /// Enable interpolation between frames
    pub interpolation_enabled: bool,
    /// Enable temporal dithering
    pub dithering_enabled: bool,
    /// Apply [`Self::white_point`] to each channel.
    ///
    /// ⚠️ The name is historical and the wire format is stuck with it. It once
    /// selected a 257-entry lookup table; that table was measured to compute
    /// nothing but `value * white_point` and was replaced by the multiply
    /// (`docs/defects/2026-08-01-classic-rmt-open-fault.md`). What the flag has
    /// always actually gated is whether the white point is applied **at all** —
    /// `false` means channels pass through unbalanced, not "balance by a
    /// cheaper method". Renaming it would break every project file on disk.
    pub lut_enabled: bool,
}

impl Default for DisplayPipelineOptions {
    fn default() -> Self {
        Self {
            white_point: [0.9, 1.0, 1.0],
            interpolation_enabled: true,
            dithering_enabled: true,
            lut_enabled: true,
        }
    }
}
