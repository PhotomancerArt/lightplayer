//! Errors for [`crate::LpvmDataQ32`] and related buffer/value conversion.

use alloc::string::String;

use lps_shared::path_resolve::PathError;

/// Failure reading/writing [`crate::LpvmDataQ32`] or converting to/from [`crate::LpsValueF32`].
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

    /// [`LpsType::Texture2D`] uses an opaque ABI descriptor; it must not be written through
    /// ordinary [`crate::encode_uniform_write`] / [`crate::LpvmDataQ32`] value paths.
    pub fn texture_uniform_requires_binding_helper() -> Self {
        Self::type_mismatch(
            "Texture2D uniform",
            "use a typed Texture2D binding/descriptor helper; ordinary uniform and LpvmDataQ32 value writes are not supported",
        )
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
