//! The project sidecar: `<project>/.lp/access.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::access_file_error::AccessFileError;
use crate::secret_entry::{SecretEntry, read_version, validate_secrets};

/// A project's own secrets, stored beside the project in its `/.lp/`
/// namespace so they travel with it on deploy.
///
/// `/.lp/` is outside the content hash (`lpc-history`'s hash rules), so
/// changing a secret never changes the project's identity, and cloud push,
/// publish and fork skip it with the rest of `/.lp/`. The file is
/// **write-only** on every link: a device pull cannot bring it back, and
/// the library copy is the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAccessFile {
    /// Format version; always [`ProjectAccessFile::VERSION`].
    pub version: u32,
    /// The project's secrets.
    pub secrets: Vec<SecretEntry>,
}

impl ProjectAccessFile {
    /// The format version this build reads and writes.
    pub const VERSION: u32 = 1;

    /// Path of the sidecar relative to a project directory.
    pub const RELATIVE_PATH: &'static str = "/.lp/access.json";

    /// A sidecar holding `secrets`, at the current version.
    #[must_use]
    pub fn new(secrets: Vec<SecretEntry>) -> Self {
        Self {
            version: Self::VERSION,
            secrets,
        }
    }

    /// Parse and validate the file's bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AccessFileError> {
        let version = read_version(bytes)?;
        if version != Self::VERSION {
            return Err(AccessFileError::UnsupportedVersion(version));
        }
        let file: Self = serde_json::from_slice(bytes)
            .map_err(|error| AccessFileError::Malformed(alloc::format!("{error}")))?;
        validate_secrets(&file.secrets)?;
        Ok(file)
    }

    /// Serialize to the file's bytes.
    pub fn to_json(&self) -> Result<String, AccessFileError> {
        serde_json::to_string(self)
            .map_err(|error| AccessFileError::Malformed(alloc::format!("{error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tier::Tier;
    use alloc::vec;

    #[test]
    fn round_trips() {
        let file = ProjectAccessFile::new(vec![SecretEntry::from_password(
            "camp",
            Tier::Play,
            b"s'mores",
            [3u8; 16],
            4,
        )]);
        let json = file.to_json().unwrap();
        assert!(json.starts_with("{\"version\":1,"), "{json}");
        assert_eq!(ProjectAccessFile::from_json(json.as_bytes()).unwrap(), file);
    }

    #[test]
    fn other_versions_are_refused_as_versions() {
        let json = b"{\"version\":2,\"secrets\":[],\"somethingNew\":true}";
        assert_eq!(
            ProjectAccessFile::from_json(json),
            Err(AccessFileError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn zero_iterations_are_refused() {
        let mut entry = SecretEntry::from_password("x", Tier::Edit, b"pw", [0u8; 16], 1);
        entry.iterations = 0;
        let json = ProjectAccessFile::new(vec![entry]).to_json().unwrap();
        assert!(matches!(
            ProjectAccessFile::from_json(json.as_bytes()),
            Err(AccessFileError::ZeroIterations { .. })
        ));
    }

    #[test]
    fn garbage_is_malformed() {
        assert!(matches!(
            ProjectAccessFile::from_json(b"not json"),
            Err(AccessFileError::Malformed(_))
        ));
    }
}
