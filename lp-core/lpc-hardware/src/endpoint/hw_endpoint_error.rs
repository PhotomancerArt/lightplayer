use alloc::string::String;
use core::fmt;

use crate::{HwEndpointId, HwEndpointKind, HwError};

/// Failure while resolving or opening a hardware endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareEndpointError {
    /// No registered driver currently exposes the requested endpoint.
    UnknownEndpoint {
        kind: HwEndpointKind,
        endpoint_id: HwEndpointId,
    },
    /// The endpoint exists but is reserved, already claimed, or not initialized.
    EndpointUnavailable {
        endpoint_id: HwEndpointId,
        reason: String,
    },
    /// The endpoint exists, but the requested configuration is unsupported.
    UnsupportedConfig { reason: String },
    /// Lower-level resource or registry failure.
    Hardware { error: HwError },
    /// Target-specific endpoint failure.
    Other { message: String },
}

impl fmt::Display for HardwareEndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEndpoint { kind, endpoint_id } => {
                write!(f, "unknown {kind:?} hardware endpoint: {endpoint_id}")
            }
            Self::EndpointUnavailable {
                endpoint_id,
                reason,
            } => {
                write!(
                    f,
                    "hardware endpoint {endpoint_id} is unavailable: {reason}"
                )
            }
            Self::UnsupportedConfig { reason } => {
                write!(f, "unsupported hardware endpoint config: {reason}")
            }
            Self::Hardware { error } => write!(f, "{error}"),
            Self::Other { message } => f.write_str(message),
        }
    }
}

impl From<HwError> for HardwareEndpointError {
    fn from(error: HwError) -> Self {
        Self::Hardware { error }
    }
}
