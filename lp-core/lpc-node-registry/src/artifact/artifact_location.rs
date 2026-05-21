//! Resolved file location used as the artifact store cache key.

use core::cmp::Ordering;

use lpc_model::{ArtifactLocator, LpPathBuf};

use super::ArtifactError;

/// Resolved load location (M1: file-backed paths only).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArtifactLocation {
    File(LpPathBuf),
}

impl ArtifactLocation {
    pub fn file(path: impl Into<LpPathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn try_from_locator(locator: &ArtifactLocator) -> Result<Self, ArtifactError> {
        match locator {
            ArtifactLocator::Path(path) => Ok(Self::File(path.clone())),
            ArtifactLocator::Lib(lib) => Err(ArtifactError::Resolution(alloc::format!(
                "library artifact references are not supported yet ({lib})"
            ))),
        }
    }

    pub fn file_path(&self) -> Option<&LpPathBuf> {
        match self {
            Self::File(path) => Some(path),
        }
    }
}

impl Ord for ArtifactLocation {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::File(a), Self::File(b)) => a.as_str().cmp(b.as_str()),
        }
    }
}

impl PartialOrd for ArtifactLocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::ArtifactError;
    use lpc_model::artifact::src_artifact_lib_ref::SrcArtifactLibRef;

    #[test]
    fn path_locator_resolves_to_file() {
        let loc = ArtifactLocator::path("./shader.glsl");
        let location = ArtifactLocation::try_from_locator(&loc).unwrap();
        assert_eq!(
            location,
            ArtifactLocation::File(LpPathBuf::from("./shader.glsl"))
        );
    }

    #[test]
    fn lib_locator_returns_resolution_error() {
        let loc = ArtifactLocator::lib_ref(
            SrcArtifactLibRef::try_from_suffix("core/x").expect("valid lib ref"),
        );
        let err = ArtifactLocation::try_from_locator(&loc).unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(msg) if msg.contains("not supported")));
    }
}
