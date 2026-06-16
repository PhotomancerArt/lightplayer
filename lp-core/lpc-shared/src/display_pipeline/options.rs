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
    /// Global brightness multiplier (0.0–1.0, default 1.0)
    pub brightness: f32,
    /// Enable interpolation between frames
    pub interpolation_enabled: bool,
    /// Enable temporal dithering
    pub dithering_enabled: bool,
    /// Enable white point LUT
    pub lut_enabled: bool,
}

impl Default for DisplayPipelineOptions {
    fn default() -> Self {
        Self {
            white_point: [0.9, 1.0, 1.0],
            brightness: 1.0,
            interpolation_enabled: true,
            dithering_enabled: true,
            lut_enabled: true,
        }
    }
}
