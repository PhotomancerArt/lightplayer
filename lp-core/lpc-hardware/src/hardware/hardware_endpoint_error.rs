use alloc::string::String;
use core::fmt;

use super::{HardwareEndpointId, HardwareEndpointKind, HardwareError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareEndpointError {
    UnknownEndpoint {
        kind: HardwareEndpointKind,
        endpoint_id: HardwareEndpointId,
    },
    EndpointUnavailable {
        endpoint_id: HardwareEndpointId,
        reason: String,
    },
    UnsupportedConfig {
        reason: String,
    },
    Hardware {
        error: HardwareError,
    },
    Other {
        message: String,
    },
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

impl From<HardwareError> for HardwareEndpointError {
    fn from(error: HardwareError) -> Self {
        Self::Hardware { error }
    }
}
