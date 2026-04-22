//! Cross-cutting domain error. Concrete error variants land as concrete artifact
//! types and binding-resolver implementations come online (M3+).

use alloc::string::String;
use core::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum DomainError {
    UnknownProperty(String),
    PropertyTypeMismatch { expected: String, actual: String },
    Other(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProperty(p) => write!(f, "unknown property: {p}"),
            Self::PropertyTypeMismatch { expected, actual } => {
                write!(
                    f,
                    "property type mismatch: expected {expected}, got {actual}"
                )
            }
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl core::error::Error for DomainError {}
