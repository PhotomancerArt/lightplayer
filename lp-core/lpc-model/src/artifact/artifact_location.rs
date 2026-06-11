//! Resolved artifact identity.

use alloc::format;
use alloc::string::String;

use crate::{ArtifactLocationError, ArtifactSpec, LpPath, LpPathBuf};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const FILE_URI_PREFIX: &str = "file:";

/// Canonical project identity for a file-backed artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactLocation {
    path: LpPathBuf,
}

impl ArtifactLocation {
    pub fn file(path: impl Into<LpPathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_absolute_path(path: LpPathBuf) -> Self {
        Self { path }
    }

    pub fn try_from_specifier(specifier: &ArtifactSpec) -> Result<Self, ArtifactLocationError> {
        match specifier {
            ArtifactSpec::Path(path) => Ok(Self::file(path.clone())),
            ArtifactSpec::Lib(lib) => Err(ArtifactLocationError::Resolution(format!(
                "library artifact references are not supported yet ({lib})"
            ))),
        }
    }

    pub fn file_path(&self) -> &LpPathBuf {
        &self.path
    }

    pub fn to_uri(&self) -> String {
        format!("{FILE_URI_PREFIX}{}", self.path.as_str())
    }

    pub fn parse_uri(raw: &str) -> Result<Self, ArtifactLocationError> {
        let raw = raw.trim();
        if let Some(rest) = raw.strip_prefix(FILE_URI_PREFIX) {
            if rest.is_empty() {
                return Err(ArtifactLocationError::Resolution(format!(
                    "invalid artifact uri `{raw}`"
                )));
            }
            return Ok(Self::file(rest));
        }
        if raw.starts_with('/') {
            return Ok(Self::file(raw));
        }
        Err(ArtifactLocationError::Resolution(format!(
            "artifact uri must start with `{FILE_URI_PREFIX}` or be an absolute path, got `{raw}`"
        )))
    }

    pub fn location_for_path(path: &LpPath) -> Self {
        Self::file(path.to_path_buf())
    }
}

impl Serialize for ArtifactLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_uri())
    }
}

impl<'de> Deserialize<'de> for ArtifactLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse_uri(&raw).map_err(|err| serde::de::Error::custom(format!("{err:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::src_artifact_lib_ref::SrcArtifactLibRef;

    #[test]
    fn path_specifier_resolves_to_file() {
        let spec = ArtifactSpec::path("./shader.glsl");
        let location = ArtifactLocation::try_from_specifier(&spec).unwrap();
        assert_eq!(location, ArtifactLocation::file("./shader.glsl"));
    }

    #[test]
    fn lib_specifier_returns_resolution_error() {
        let spec = ArtifactSpec::lib_ref(
            SrcArtifactLibRef::try_from_suffix("core/x").expect("valid lib ref"),
        );
        let err = ArtifactLocation::try_from_specifier(&spec).unwrap_err();
        assert!(
            matches!(err, ArtifactLocationError::Resolution(msg) if msg.contains("not supported"))
        );
    }

    #[test]
    fn uri_roundtrip_and_serde() {
        let location = ArtifactLocation::file("/shader.toml");
        assert_eq!(location.to_uri(), "file:/shader.toml");
        assert_eq!(
            ArtifactLocation::parse_uri("file:/shader.toml").unwrap(),
            location
        );
        let json = serde_json::to_string(&location).unwrap();
        assert_eq!(json, "\"file:/shader.toml\"");
        let back: ArtifactLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, location);
    }

    #[test]
    fn parse_absolute_path_without_prefix() {
        let location = ArtifactLocation::parse_uri("/shader.toml").unwrap();
        assert_eq!(location, ArtifactLocation::file("/shader.toml"));
    }
}
