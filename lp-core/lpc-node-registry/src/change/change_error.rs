//! Errors from applying client changes to the overlay.

use alloc::string::String;
use core::fmt;

/// Failure applying an [`super::ArtifactChange`] or [`super::ChangeSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeError {
    InvalidPath { message: String },
    UnknownArtifact { artifact_id: u32 },
    UnsupportedOp { op: &'static str },
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { message } => write!(f, "invalid path: {message}"),
            Self::UnknownArtifact { artifact_id } => {
                write!(f, "unknown artifact id {artifact_id}")
            }
            Self::UnsupportedOp { op } => write!(f, "unsupported change op: {op}"),
        }
    }
}
