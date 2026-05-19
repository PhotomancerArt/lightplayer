//! Resolved runtime location for loading and caching artifacts.

use core::cmp::Ordering;

use alloc::string::String;
use lpc_model::{ArtifactLocator, LpPathBuf};

/// Resolved load location used as the artifact manager cache key.
///
/// `ArtifactLocator` is authored and context-dependent. `ArtifactLocation`
/// is the engine-side resolved address that can be loaded and cached. It is
/// deliberately separate from the authored locator so relative paths, future
/// libraries, and built-ins can all resolve into stable runtime identities.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArtifactLocation {
    File(LpPathBuf),
    InlineNode { owner: LpPathBuf, name: String },
}

impl ArtifactLocation {
    pub fn file(path: impl Into<LpPathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn inline_node(owner: impl Into<LpPathBuf>, name: impl Into<String>) -> Self {
        Self::InlineNode {
            owner: owner.into(),
            name: name.into(),
        }
    }

    pub fn try_from_src_spec(spec: &ArtifactLocator) -> Result<Self, super::ArtifactError> {
        match spec {
            ArtifactLocator::Path(path) => Ok(Self::File(path.clone())),
            ArtifactLocator::Lib(lib) => Err(super::ArtifactError::Resolution(alloc::format!(
                "library artifact references are not supported yet ({lib})"
            ))),
        }
    }
}

impl Ord for ArtifactLocation {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::File(a), Self::File(b)) => a.as_str().cmp(b.as_str()),
            (Self::File(_), Self::InlineNode { .. }) => Ordering::Less,
            (Self::InlineNode { .. }, Self::File(_)) => Ordering::Greater,
            (
                Self::InlineNode {
                    owner: a_owner,
                    name: a_name,
                },
                Self::InlineNode {
                    owner: b_owner,
                    name: b_name,
                },
            ) => a_owner
                .as_str()
                .cmp(b_owner.as_str())
                .then_with(|| a_name.cmp(b_name)),
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
        let spec = ArtifactLocator::path("./fx/../fx/a.effect.toml");
        let location = ArtifactLocation::try_from_src_spec(&spec).unwrap();
        match location {
            ArtifactLocation::File(path) => assert_eq!(path.as_str(), "fx/../fx/a.effect.toml"),
            ArtifactLocation::InlineNode { .. } => panic!("expected file location"),
        }
    }

    #[test]
    fn try_from_src_spec_rejects_lib_for_now() {
        let spec = ArtifactLocator::parse("lib:core/x").unwrap();
        let err = ArtifactLocation::try_from_src_spec(&spec).unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(s) if s.contains("not supported")));
    }
}
