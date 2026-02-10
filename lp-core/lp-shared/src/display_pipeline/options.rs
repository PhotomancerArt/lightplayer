//! Display pipeline options

/// Options for display pipeline (LUT, dithering, interpolation)
#[derive(Debug, Clone)]
pub struct DisplayPipelineOptions {
    /// Gamma exponent for luminance curve (default 2)
    pub lum_power: f32,
    /// RGB white point balance
    pub white_point: [f32; 3],
    /// Global brightness multiplier (0.0–1.0, default 1.0)
    pub brightness: f32,
    /// Enable interpolation between frames
    pub interpolation_enabled: bool,
    /// Enable temporal dithering
    pub dithering_enabled: bool,
    /// Enable gamma + white point LUT
    pub lut_enabled: bool,
}

impl Default for DisplayPipelineOptions {
    fn default() -> Self {
        Self {
            lum_power: 2.0,
            white_point: [0.9, 1.0, 1.0],
            brightness: 1.0,
            interpolation_enabled: true,
            dithering_enabled: true,
            lut_enabled: true,
        }
    }
}
