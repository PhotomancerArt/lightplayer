//! Error types for lpc-shared

use alloc::string::String;
use core::fmt;

/// Texture error type
#[derive(Debug, Clone)]
pub enum TextureError {
    /// Invalid texture format
    InvalidFormat(String),
    /// Texture dimensions too large
    DimensionsTooLarge { width: u32, height: u32 },
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureError::InvalidFormat(format) => {
                write!(f, "Invalid texture format: {format}")
            }
            TextureError::DimensionsTooLarge { width, height } => {
                write!(f, "Texture dimensions too large: {width}x{height}")
            }
        }
    }
}

/// Display pipeline error type
#[derive(Debug, Clone)]
pub enum DisplayPipelineError {
    /// Allocation failed (e.g. too many LEDs)
    AllocationFailed { num_leds: u32 },
}

impl fmt::Display for DisplayPipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayPipelineError::AllocationFailed { num_leds } => {
                write!(f, "DisplayPipeline allocation failed for {num_leds} LEDs")
            }
        }
    }
}
