//! Errors for parsing, validating, resolving, and fitting mapping documents.

use alloc::string::String;

#[derive(Debug, Clone, PartialEq)]
pub enum Map2dError {
    /// The document is not valid JSON for the schema.
    Parse(String),
    /// The document's `format` is zero or newer than this crate supports.
    UnsupportedFormat { found: u32, supported: u32 },
    /// Two objects claim the same stable id — patch entries addressing it
    /// would be ambiguous, so the document is refused whole.
    DuplicateObjectId { id: String },
    /// One object's parameters are invalid; `object` is its wiring-order
    /// index and `name` its authored name (may be empty).
    InvalidObject {
        object: u32,
        name: String,
        reason: String,
    },
    /// Fit was asked for but the geometry (or target) has no usable extent.
    EmptyBounds,
}

impl core::fmt::Display for Map2dError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "invalid map2d document: {reason}"),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "unsupported map2d format {found} (this build reads up to {supported})"
            ),
            Self::DuplicateObjectId { id } => {
                write!(f, "map2d document claims object id {id:?} more than once")
            }
            Self::InvalidObject {
                object,
                name,
                reason,
            } => {
                if name.is_empty() {
                    write!(f, "map2d object {object}: {reason}")
                } else {
                    write!(f, "map2d object {object} ({name:?}): {reason}")
                }
            }
            Self::EmptyBounds => write!(f, "map2d geometry has no usable extent to fit"),
        }
    }
}

impl core::error::Error for Map2dError {}
