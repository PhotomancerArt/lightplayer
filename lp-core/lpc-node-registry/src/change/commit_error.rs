//! Errors from promoting overlay to committed state.

use alloc::string::String;
use core::fmt;

/// Failure during [`super::NodeDefRegistry::commit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitError {
    Fs { message: String },
    Serialize { message: String },
    Registry { message: String },
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fs { message } => write!(f, "filesystem error: {message}"),
            Self::Serialize { message } => write!(f, "serialize error: {message}"),
            Self::Registry { message } => write!(f, "registry error: {message}"),
        }
    }
}

impl From<crate::change::ChangeError> for CommitError {
    fn from(err: crate::change::ChangeError) -> Self {
        match err {
            crate::change::ChangeError::Serialize { message } => Self::Serialize { message },
            other => Self::Registry {
                message: alloc::format!("{other}"),
            },
        }
    }
}

impl From<crate::RegistryError> for CommitError {
    fn from(err: crate::RegistryError) -> Self {
        Self::Registry {
            message: alloc::format!("{err:?}"),
        }
    }
}
