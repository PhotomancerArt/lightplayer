//! The `lp.package` envelope: a whole project as pasteable JSON.
//!
//! The same file set the zip codec carries
//! ([`crate::app::library::export_package`]) — every package file including
//! `/.lp/meta.json`, never `/history/**` — in a form that survives a chat
//! window. Zip is still the right channel for a real handoff; JSON is for
//! the small project you want to paste into a message.
//!
//! Import mints a **fresh uid**, exactly as zip import does: envelopes get
//! shared, and two libraries holding the same uid would break the identity
//! that history and device associations key off.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::share_envelope::{
    PACKAGE_KIND, SHARE_FORMAT_VERSION, check_format, check_kind, peek_header_lenient,
};
use super::share_error::ShareError;
use super::share_file::ShareFile;

/// A whole project, ready for the clipboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageEnvelope {
    pub kind: String,
    pub format: u32,
    /// The project's display name, for the paste target's confirmation and
    /// the installed package's label.
    pub name: String,
    /// Package-relative path → contents. A `BTreeMap` so the encoding is
    /// deterministic: the same project always produces byte-identical
    /// text, which makes a pasted envelope diffable.
    pub files: BTreeMap<String, ShareFile>,
}

impl PackageEnvelope {
    /// Wrap a package's files. `files` is the sorted relative-path list
    /// [`crate::app::library::PackageHandle::read_all_files`] returns, so
    /// the zip and JSON paths share one snapshot.
    pub fn encode(name: &str, files: &[(String, Vec<u8>)]) -> Self {
        Self {
            kind: PACKAGE_KIND.to_string(),
            format: SHARE_FORMAT_VERSION,
            name: name.to_string(),
            files: files
                .iter()
                .map(|(path, bytes)| (path.clone(), ShareFile::from_bytes(bytes)))
                .collect(),
        }
    }

    /// Parse an envelope from pasted text.
    ///
    /// The header is validated **before** the body is deserialized, so
    /// pasting a node envelope here reports "that is an lp.node" rather
    /// than a structural complaint about a missing `name` field.
    pub fn decode(text: &str) -> Result<Self, ShareError> {
        let header = peek_header_lenient(text)?;
        check_kind(&header.kind, &[PACKAGE_KIND])?;
        check_format(header.format)?;

        let envelope: Self =
            serde_json::from_str(text).map_err(|error| ShareError::Malformed(error.to_string()))?;
        if !envelope.files.contains_key("project.json") {
            return Err(ShareError::Malformed(
                "no project.json in this envelope".to_string(),
            ));
        }
        Ok(envelope)
    }

    /// Serialize for the clipboard.
    pub fn to_json(&self) -> Result<String, ShareError> {
        serde_json::to_string_pretty(self).map_err(|error| ShareError::Malformed(error.to_string()))
    }

    /// The file list in the shape the library installer takes.
    pub fn into_files(self) -> Result<Vec<(String, Vec<u8>)>, ShareError> {
        self.files
            .into_iter()
            .map(|(path, file)| {
                let bytes = file.to_bytes(&path)?;
                Ok((path, bytes))
            })
            .collect()
    }

    /// The envelope's own uid, when it carried one. Rides the installed
    /// package's provenance so a shared copy remembers its source, exactly
    /// as `ImportedZip` does.
    pub fn original_uid(&self) -> Option<String> {
        let ShareFile::Text { text } = self.files.get("project.json")? else {
            return None;
        };
        serde_json::from_str::<serde_json::Value>(text.as_str())
            .ok()?
            .get("uid")?
            .as_str()
            .map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_round_trips_byte_for_byte() {
        // Path-sorted, matching what `PackageHandle::read_all_files`
        // returns — the envelope's `BTreeMap` re-sorts regardless, which is
        // what `encoding_is_deterministic_regardless_of_input_order` pins.
        let files = vec![
            ("logo.png".to_string(), vec![0x89, b'P', b'N', b'G', 0x00]),
            ("orbit.glsl".to_string(), b"void main() {}".to_vec()),
            (
                "project.json".to_string(),
                br#"{"kind":"Module"}"#.to_vec(),
            ),
        ];
        let json = PackageEnvelope::encode("Demo", &files).to_json().unwrap();

        let decoded = PackageEnvelope::decode(&json).unwrap();
        assert_eq!(decoded.name, "Demo");
        assert_eq!(decoded.into_files().unwrap(), files);
    }

    #[test]
    fn text_files_stay_readable_in_the_encoded_json() {
        // The reason JSON exists alongside zip: you can read the thing you
        // pasted. If this regresses, the channel loses its point.
        let files = vec![
            (
                "project.json".to_string(),
                br#"{"kind":"Module"}"#.to_vec(),
            ),
            ("orbit.glsl".to_string(), b"void main() {}".to_vec()),
        ];
        let json = PackageEnvelope::encode("Demo", &files).to_json().unwrap();
        assert!(json.contains("void main()"), "{json}");
        // ...and the binary one does not masquerade as text.
        let binary = vec![("project.json".to_string(), vec![0x89, 0xff])];
        let json = PackageEnvelope::encode("Demo", &binary).to_json().unwrap();
        assert!(json.contains("base64"), "{json}");
    }

    #[test]
    fn encoding_is_deterministic_regardless_of_input_order() {
        let forward = vec![
            ("a.json".to_string(), b"a".to_vec()),
            ("project.json".to_string(), b"p".to_vec()),
        ];
        let reversed = vec![
            ("project.json".to_string(), b"p".to_vec()),
            ("a.json".to_string(), b"a".to_vec()),
        ];
        assert_eq!(
            PackageEnvelope::encode("Demo", &forward).to_json().unwrap(),
            PackageEnvelope::encode("Demo", &reversed)
                .to_json()
                .unwrap()
        );
    }

    #[test]
    fn an_envelope_without_a_manifest_is_rejected() {
        // Matches `import_zip`'s rule: no project.json, no project.
        let json = PackageEnvelope::encode("Demo", &[("a.glsl".to_string(), b"x".to_vec())])
            .to_json()
            .unwrap();
        let error = PackageEnvelope::decode(&json).unwrap_err();
        assert!(error.to_string().contains("project.json"), "{error}");
    }

    #[test]
    fn a_node_envelope_is_refused_by_the_package_decoder() {
        let json = r#"{"kind":"lp.node","format":1,"files":{}}"#;
        assert_eq!(
            PackageEnvelope::decode(json).unwrap_err(),
            ShareError::UnknownKind {
                kind: "lp.node".to_string()
            }
        );
    }

    #[test]
    fn a_future_format_is_rejected_rather_than_migrated() {
        let json = r#"{"kind":"lp.package","format":9,"name":"x","files":{}}"#;
        assert_eq!(
            PackageEnvelope::decode(json).unwrap_err(),
            ShareError::UnsupportedFormat {
                found: 9,
                supported: 1
            }
        );
    }

    #[test]
    fn the_source_uid_rides_along_for_provenance() {
        let files = vec![(
            "project.json".to_string(),
            br#"{"kind":"Module","uid":"prj_abc123"}"#.to_vec(),
        )];
        let envelope = PackageEnvelope::encode("Demo", &files);
        assert_eq!(envelope.original_uid().as_deref(), Some("prj_abc123"));

        // A project that never entered a library has no uid to carry.
        let anonymous = PackageEnvelope::encode(
            "Demo",
            &[(
                "project.json".to_string(),
                br#"{"kind":"Module"}"#.to_vec(),
            )],
        );
        assert_eq!(anonymous.original_uid(), None);
    }
}
