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

    pub fn file_from_spec(spec: &SrcArtifactSpec) -> Self {
        Self::file(spec.0.as_str())
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
    use alloc::string::String;

    #[test]
    fn file_from_spec_preserves_file_path_location() {
        let spec = SrcArtifactSpec(String::from("./fx/../fx/a.effect.toml"));
        let location = ArtifactLocation::file_from_spec(&spec);
        match location {
            ArtifactLocation::File(path) => assert_eq!(path.as_str(), "fx/../fx/a.effect.toml"),
        }
    }
}
