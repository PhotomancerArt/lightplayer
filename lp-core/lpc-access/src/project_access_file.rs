//! The project sidecar: `<project>/.lp/access.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::access_file_error::AccessFileError;
use crate::secret_entry::{SecretEntry, SecretEntryV1, read_version, validate_secrets};

/// A project's own secrets, stored beside the project in its `/.lp/`
/// namespace so they travel with it on deploy.
///
/// `/.lp/` is outside the content hash (`lpc-history`'s hash rules), so
/// changing a secret never changes the project's identity, and cloud push,
/// publish and fork skip it with the rest of `/.lp/`. The file is
/// **write-only** on every link: a device pull cannot bring it back, and
/// the library copy is the source.
///
/// Version 2 (this shape) added each entry's `kind` and `addedAt`. A
/// version-1 sidecar still reads — its entries become `kind: password`
/// with no time — and is written back as version 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectAccessFile {
    /// Format version; always [`ProjectAccessFile::VERSION`] when written.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 2, max = 2)))]
    pub version: u32,
    /// The project's secrets.
    pub secrets: Vec<SecretEntry>,
}

/// The version-1 sidecar shape, read and converted, never written.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectAccessFileV1 {
    /// Checked by `read_version` before this parse.
    #[serde(rename = "version")]
    _version: u32,
    secrets: Vec<SecretEntryV1>,
}

impl ProjectAccessFile {
    /// The format version this build writes.
    pub const VERSION: u32 = 2;

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

    /// Parse and validate the file's bytes, version 1 or 2.
    pub fn from_json(bytes: &[u8]) -> Result<Self, AccessFileError> {
        let file = match read_version(bytes)? {
            Self::VERSION => serde_json::from_slice::<Self>(bytes).map_err(malformed)?,
            1 => {
                let v1: ProjectAccessFileV1 = serde_json::from_slice(bytes).map_err(malformed)?;
                Self::new(v1.secrets.into_iter().map(SecretEntry::from).collect())
            }
            other => return Err(AccessFileError::UnsupportedVersion(other)),
        };
        validate_secrets(&file.secrets)?;
        Ok(file)
    }

    /// Serialize to the file's bytes, always at [`Self::VERSION`].
    pub fn to_json(&self) -> Result<String, AccessFileError> {
        serde_json::to_string(&Self::new(self.secrets.clone())).map_err(malformed)
    }
}

fn malformed(error: serde_json::Error) -> AccessFileError {
    AccessFileError::Malformed(alloc::format!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_kind::SecretKind;
    use crate::tier::Tier;
    use alloc::vec;

    /// A version-1 sidecar exactly as the v1 writer produced it: `camp`,
    /// `s'mores`, salt 3s, 4 iterations (the entry `round_trips` builds).
    const V1_SIDECAR: &str = "{\"version\":1,\"secrets\":[{\"label\":\"camp\",\"tier\":\"play\",\
        \"salt\":\"AwMDAwMDAwMDAwMDAwMDAw==\",\"iterations\":4,\
        \"k\":\"W5prtgAY854yezF61pjklQ+VhgWxkwUBZhZUq4xSCqA=\"}]}";

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
        assert!(json.starts_with("{\"version\":2,"), "{json}");
        assert_eq!(ProjectAccessFile::from_json(json.as_bytes()).unwrap(), file);
    }

    #[test]
    fn a_v1_sidecar_reads_as_v2_passwords() {
        let file = ProjectAccessFile::from_json(V1_SIDECAR.as_bytes()).unwrap();
        let expected = SecretEntry::from_password("camp", Tier::Play, b"s'mores", [3u8; 16], 4);
        assert_eq!(file, ProjectAccessFile::new(vec![expected]));
        assert_eq!(file.secrets[0].kind, SecretKind::Password);
        assert!(file.to_json().unwrap().starts_with("{\"version\":2,"));
    }

    #[test]
    fn other_versions_are_refused_as_versions() {
        let json = b"{\"version\":3,\"secrets\":[],\"somethingNew\":true}";
        assert_eq!(
            ProjectAccessFile::from_json(json),
            Err(AccessFileError::UnsupportedVersion(3))
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
