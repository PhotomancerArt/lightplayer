//! Errors returned by [`super::NodeDefRegistry`].

use alloc::string::String;

use crate::ArtifactError;

/// Registry operation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    NotEmpty,
    InvalidPath { message: String },
    DuplicateDefLocation,
    UnknownDef,
    SpecifierResolution { message: String },
    Utf8 { message: String },
    Artifact(ArtifactError),
}

impl From<ArtifactError> for RegistryError {
    fn from(err: ArtifactError) -> Self {
        Self::Artifact(err)
    }
}
