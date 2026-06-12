use alloc::string::String;
use core::fmt;

use crate::hardware::HardwareError;

/// Output provider error type.
#[derive(Debug, Clone)]
pub enum OutputError {
    /// Pin is already open.
    PinAlreadyOpen { pin: u32 },
    /// Hardware resource claim failed.
    Hardware { error: HardwareError },
    /// Invalid handle.
    InvalidHandle { handle: i32 },
    /// Invalid configuration.
    InvalidConfig { reason: String },
    /// Data length mismatch.
    DataLengthMismatch { expected: u32, actual: usize },
    /// Other error.
    Other { message: String },
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::PinAlreadyOpen { pin } => {
                write!(f, "Pin {pin} is already open")
            }
            OutputError::Hardware { error } => {
                write!(f, "Hardware error: {error}")
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
