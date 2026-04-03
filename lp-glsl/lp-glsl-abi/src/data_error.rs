//! Errors for [`crate::GlslData`] and related buffer/value conversion.

use alloc::string::String;

use crate::path_resolve::PathError;

/// Failure reading/writing [`crate::GlslData`] or converting to/from [`crate::GlslValue`].
#[derive(Debug)]
pub enum GlslDataError {
    Path(PathError),
    LayoutNotImplemented,
    BufferTooShort { need: usize, have: usize },
    TypeMismatch { expected: String, message: String },
    BadPathForScalar { path: String, expected_ty: String },
}

impl GlslDataError {
    pub fn type_mismatch(expected: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TypeMismatch {
            expected: expected.into(),
            message: message.into(),
        }
    }
}

impl core::fmt::Display for GlslDataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Path(e) => write!(f, "{e}"),
            Self::LayoutNotImplemented => write!(f, "layout rules are not implemented"),
            Self::BufferTooShort { need, have } => {
                write!(f, "buffer too short: need {need} bytes, have {have}")
            }
            Self::TypeMismatch { expected, message } => {
                write!(f, "type mismatch (expected {expected}): {message}")
            }
            Self::BadPathForScalar { path, expected_ty } => write!(
                f,
                "path `{path}` does not resolve to scalar type `{expected_ty}` for this operation"
            ),
        }
    }
}

impl core::error::Error for GlslDataError {}

impl From<PathError> for GlslDataError {
    fn from(e: PathError) -> Self {
        Self::Path(e)
    }
}
