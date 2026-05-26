//! Resolved artifact identity (catalog key and wire URI).

use alloc::format;
use alloc::string::String;
use core::cmp::Ordering;

use lpc_model::{ArtifactSpecifier, LpPathBuf};
use lpfs::LpPath as LpFsPath;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ArtifactError;

const FILE_URI_PREFIX: &str = "file:";

/// Resolved artifact location — canonical project identity for file-backed artifacts.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArtifactLocation {
    File(LpPathBuf),
}

impl ArtifactLocation {
    pub fn file(path: impl Into<LpPathBuf>) -> Self {
        Self::File(path.into())
    }

    pub fn from_absolute_path(path: LpPathBuf) -> Self {
        Self::File(path)
    }

    pub fn try_from_specifier(specifier: &ArtifactSpecifier) -> Result<Self, ArtifactError> {
        match specifier {
            ArtifactSpecifier::Path(path) => Ok(Self::File(path.clone())),
            ArtifactSpecifier::Lib(lib) => Err(ArtifactError::Resolution(format!(
                "library artifact references are not supported yet ({lib})"
            ))),
        }
    }

    pub fn file_path(&self) -> Option<&LpPathBuf> {
        match self {
            Self::File(path) => Some(path),
        }
    }

    pub fn to_uri(&self) -> String {
        match self {
            Self::File(path) => format!("{FILE_URI_PREFIX}{}", path.as_str()),
        }
    }

    pub fn parse_uri(raw: &str) -> Result<Self, ArtifactError> {
        let raw = raw.trim();
        if let Some(rest) = raw.strip_prefix(FILE_URI_PREFIX) {
            if rest.is_empty() {
                return Err(ArtifactError::Resolution(format!(
                    "invalid artifact uri `{raw}`"
                )));
            }
            return Ok(Self::File(LpPathBuf::from(rest)));
        }
        if raw.starts_with('/') {
            return Ok(Self::File(LpPathBuf::from(raw)));
        }
        Err(ArtifactError::Resolution(format!(
            "artifact uri must start with `{FILE_URI_PREFIX}` or be an absolute path, got `{raw}`"
        )))
    }

    pub fn location_for_path(path: &LpFsPath) -> Self {
        Self::File(path.to_path_buf())
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
    use lpc_model::artifact::src_artifact_lib_ref::SrcArtifactLibRef;

    #[test]
    fn path_specifier_resolves_to_file() {
        let spec = ArtifactSpecifier::path("./shader.glsl");
        let location = ArtifactLocation::try_from_specifier(&spec).unwrap();
        assert_eq!(
            location,
            ArtifactLocation::File(LpPathBuf::from("./shader.glsl"))
        );
    }

    #[test]
    fn lib_specifier_returns_resolution_error() {
        let spec = ArtifactSpecifier::lib_ref(
            SrcArtifactLibRef::try_from_suffix("core/x").expect("valid lib ref"),
        );
        let err = ArtifactLocation::try_from_specifier(&spec).unwrap_err();
        assert!(matches!(err, ArtifactError::Resolution(msg) if msg.contains("not supported")));
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
