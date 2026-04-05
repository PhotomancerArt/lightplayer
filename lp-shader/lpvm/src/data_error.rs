//! Errors for [`crate::LpvmData`] and related buffer/value conversion.

use alloc::string::String;

use lps_shared::path_resolve::PathError;

/// Failure reading/writing [`crate::LpvmData`] or converting to/from [`crate::LpsValue`].
#[derive(Debug)]
pub enum DataError {
    Path(PathError),
    LayoutNotImplemented,
    BufferTooShort { need: usize, have: usize },
    TypeMismatch { expected: String, message: String },
    BadPathForScalar { path: String, expected_ty: String },
}

impl DataError {
    pub fn type_mismatch(expected: impl Into<String>, message: impl Into<String>) -> Self {
        Self::TypeMismatch {
            expected: expected.into(),
            message: message.into(),
        }
    }
}

impl core::fmt::Display for DataError {
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

impl core::error::Error for DataError {}

impl From<PathError> for DataError {
    fn from(e: PathError) -> Self {
        Self::Path(e)
    }
}
