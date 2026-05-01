//! Resolved runtime location for loading and caching artifacts.

use core::cmp::Ordering;

use lpc_model::LpPathBuf;
use lpc_source::SrcArtifactSpec;

/// Resolved load location used as the artifact manager cache key.
///
/// `SrcArtifactSpec` is authored and context-dependent. `ArtifactLocation`
/// is the engine-side resolved address that can be loaded and cached. M4.3
/// supports file-backed locations; built-ins and libraries can extend this
/// enum later without changing `ArtifactEntry` identity.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArtifactLocation {
    File(LpPathBuf),
}

impl ArtifactLocation {
    pub fn file(path: impl Into<LpPathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn try_from_src_spec(spec: &SrcArtifactSpec) -> Result<Self, super::ArtifactError> {
        match spec {
            SrcArtifactSpec::Path(path) => Ok(Self::File(path.clone())),
            SrcArtifactSpec::Lib(lib) => Err(super::ArtifactError::Resolution(alloc::format!(
                "library artifact references are not supported yet ({lib})"
            ))),
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
    #[test]
    fn try_from_src_spec_preserves_file_path_location() {
        let spec = SrcArtifactSpec::path("./fx/../fx/a.effect.toml");
        let location = ArtifactLocation::try_from_src_spec(&spec).unwrap();
        match location {
            ArtifactLocation::File(path) => assert_eq!(path.as_str(), "fx/../fx/a.effect.toml"),
        }
    }

    #[test]
    fn try_from_src_spec_rejects_lib_for_now() {
        let spec = SrcArtifactSpec::parse("lib:core/x").unwrap();
        let err = ArtifactLocation::try_from_src_spec(&spec).unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(s) if s.contains("not supported")));
    }
}
