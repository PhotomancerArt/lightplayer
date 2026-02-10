use crate::nodes::{NodeConfig, NodeKind};
use serde::{Deserialize, Serialize};

/// Output driver options (display pipeline). JSON-serializable config for DisplayPipelineOptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputDriverOptionsConfig {
    /// Gamma exponent for luminance curve
    #[serde(default = "default_lum_power")]
    pub lum_power: f32,
    /// RGB white point balance
    #[serde(default = "default_white_point")]
    pub white_point: [f32; 3],
    /// Global brightness multiplier (0.0–1.0)
    #[serde(default = "default_brightness")]
    pub brightness: f32,
    /// Enable interpolation between frames
    #[serde(default = "default_true")]
    pub interpolation_enabled: bool,
    /// Enable temporal dithering
    #[serde(default = "default_true")]
    pub dithering_enabled: bool,
    /// Enable gamma + white point LUT
    #[serde(default = "default_true")]
    pub lut_enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_lum_power() -> f32 {
    2.0
}
fn default_white_point() -> [f32; 3] {
    [0.9, 1.0, 1.0]
}
fn default_brightness() -> f32 {
    1.0
}

impl Default for OutputDriverOptionsConfig {
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

/// Output node configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OutputConfig {
    /// GPIO strip output
    GpioStrip {
        pin: u32,
        /// Optional display pipeline options (JSON key: "options")
        #[serde(default)]
        options: Option<OutputDriverOptionsConfig>,
    },
}

impl NodeConfig for OutputConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Output
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_config_kind() {
        let config = OutputConfig::GpioStrip {
            pin: 18,
            options: None,
        };
        assert_eq!(config.kind(), NodeKind::Output);
    }

    #[test]
    fn test_output_config_with_options_deserialize() {
        let json = r#"{"GpioStrip": {"pin": 18, "options": {"interpolation_enabled": true, "dithering_enabled": true, "lut_enabled": true, "brightness": 0.25}}}"#;
        let config: OutputConfig = crate::json::from_str(json).unwrap();
        match &config {
            OutputConfig::GpioStrip { pin, options } => {
                assert_eq!(*pin, 18);
                let opts = options.as_ref().unwrap();
                assert!(opts.interpolation_enabled);
                assert!(opts.dithering_enabled);
                assert!(opts.lut_enabled);
                assert!((opts.brightness - 0.25).abs() < 0.001);
            }
        }
    }
}
