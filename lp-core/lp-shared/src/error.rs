//! Error types for lp-shared

use alloc::string::String;
use core::fmt;

/// Filesystem error type
#[derive(Debug, Clone)]
pub enum FsError {
    /// Filesystem operation failed
    Filesystem(String),
    /// File not found
    NotFound(String),
    /// Invalid path
    InvalidPath(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::Filesystem(msg) => write!(f, "Filesystem error: {msg}"),
            FsError::NotFound(msg) => write!(f, "File not found: {msg}"),
            FsError::InvalidPath(msg) => write!(f, "Invalid path: {msg}"),
        }
    }
}

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

/// Output provider error type
#[derive(Debug, Clone)]
pub enum OutputError {
    /// Pin is already open
    PinAlreadyOpen { pin: u32 },
    /// Invalid handle
    InvalidHandle { handle: i32 },
    /// Invalid configuration
    InvalidConfig { reason: String },
    /// Data length mismatch
    DataLengthMismatch { expected: u32, actual: usize },
    /// Other error
    Other { message: String },
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::PinAlreadyOpen { pin } => {
                write!(f, "Pin {pin} is already open")
            }
            OutputError::InvalidHandle { handle } => {
                write!(f, "Invalid handle: {handle}")
            }
            OutputError::InvalidConfig { reason } => {
                write!(f, "Invalid config: {reason}")
            }
            OutputError::DataLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "Data length {actual} doesn't match expected byte_count {expected}"
                )
            }
            OutputError::Other { message } => {
                write!(f, "Error: {message}")
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
