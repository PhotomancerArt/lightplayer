use std::fmt::{self, Display};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkError {
    EndpointNotFound { endpoint: String },
    OperationUnsupported { operation: String },
    ConnectionFailed { message: String },
    Closed,
    Other { message: String },
}

impl LinkError {
    pub fn endpoint_not_found(endpoint: impl Into<String>) -> Self {
        Self::EndpointNotFound {
            endpoint: endpoint.into(),
        }
    }

    pub fn unsupported(operation: impl Into<String>) -> Self {
        Self::OperationUnsupported {
            operation: operation.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }
}

impl Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndpointNotFound { endpoint } => {
                write!(f, "link endpoint not found: {endpoint}")
            }
            Self::OperationUnsupported { operation } => {
                write!(f, "link operation unsupported: {operation}")
            }
            Self::ConnectionFailed { message } => write!(f, "link connection failed: {message}"),
            Self::Closed => write!(f, "link session is closed"),
            Self::Other { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for LinkError {}
