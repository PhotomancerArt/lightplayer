use alloc::string::String;
use core::fmt;

use crate::HwError;

/// Output-channel error type shared by hardware and higher-level providers.
///
/// The hardware crate owns this small error so opened outputs such as
/// [`crate::Ws281xOutput`] can report invalid writes without depending on the
/// engine/output provider layer.
#[derive(Debug, Clone)]
pub enum OutputError {
    /// Pin is already open.
    PinAlreadyOpen { pin: u32 },
    /// Hardware resource claim failed.
    Hardware { error: HwError },
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
