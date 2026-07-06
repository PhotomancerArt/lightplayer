//! Errors returned by project registry helpers.

use alloc::string::String;

/// Shared registry error for artifact/reference resolution during rebuild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidPath {
        message: String,
    },
    SpecifierResolution {
        message: String,
    },
    /// Project root `format` is missing or unsupported; see
    /// [`lpc_model::PROJECT_FORMAT_VERSION`].
    FormatVersion {
        expected: u32,
        found: Option<u32>,
    },
}

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPath { message } => write!(f, "invalid path: {message}"),
            Self::SpecifierResolution { message } => {
                write!(f, "specifier resolution: {message}")
            }
            Self::FormatVersion {
                expected,
                found: Some(found),
            } => {
                write!(
                    f,
                    "unsupported project format {found} (expected {expected}); \
                     regenerate or upgrade the project"
                )
            }
            Self::FormatVersion {
                expected,
                found: None,
            } => {
                write!(
                    f,
                    "project root is missing the top-level `format` key \
                     (expected {expected}); regenerate or upgrade the project"
                )
            }
        }
    }
}

impl core::error::Error for RegistryError {}
