//! The release index, format 1: every release one target can install.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::lower_hex::{BUILD_ID_COMMIT_DIGITS, COMMIT_HEX_LEN, is_lower_hex};
use crate::ota_manifest::{OtaManifest, Requires};
use crate::release_index_error::ReleaseIndexError;
use crate::release_version::ReleaseVersion;
use crate::target_name::{TargetName, is_target_name};

/// The only `format` this crate reads and writes.
pub const RELEASE_INDEX_FORMAT: u32 = 1;

/// The release index, **format 1**: the releases one target can install,
/// newest first. Served at `/api/v1/firmware/<target>/releases`
/// ([`release_index_path`](crate::release_index_path)); its schema is
/// `schemas/firmware-release-index.schema.json`.
///
/// Computed by the server from the releases it can see, never stored — but
/// Studios in the field read it, so it is a public format like
/// `ota-manifest.json`.
///
/// # Compatibility (a public contract from its first deploy)
///
/// - `format` is `1`. Readers **refuse** another `format` and **ignore
///   unknown fields**, at the top, in every entry and in its `requires` (no
///   `deny_unknown_fields`; the schema says `additionalProperties: true`).
///   That is what lets the index grow without a new format.
/// - An additive optional field keeps `format: 1`.
/// - `version`, `commit` and `target` are spelled exactly as the release's
///   `ota-manifest.json` spells them, and are never re-rendered.
/// - There is no `buildId` field: it is derived
///   ([`ReleaseIndexEntry::build_id`]), as in the manifest.
///
/// An entry is listed only when its release can be installed: the server
/// lists a release once every file its manifest names is uploaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema-gen",
    schemars(
        title = "firmware release index",
        description = "The firmware release index, format 1 (lpc_firmware_release::ReleaseIndex), served at /api/v1/firmware/<target>/releases: the releases one target can install, newest first by numeric version order. Readers refuse another format and ignore unknown fields; an additive optional field keeps format 1. See the firmware-distribution ADR.",
        extend("additionalProperties" = true)
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseIndex {
    /// Shape version; always `1` ([`RELEASE_INDEX_FORMAT`]).
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1, max = 1)))]
    pub format: u32,
    /// The target every entry is a build of (`esp32c6-4mb`); opaque.
    #[cfg_attr(
        feature = "schema-gen",
        schemars(regex(pattern = r"^[a-z0-9][a-z0-9-]{0,63}$"))
    )]
    pub target: String,
    /// The installable releases, newest first (numeric version order).
    pub releases: Vec<ReleaseIndexEntry>,
}

/// One installable release of the index's target: what the release's
/// `ota-manifest.json` says about its identity and what it needs, and when
/// it was published.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema-gen", schemars(extend("additionalProperties" = true)))]
#[serde(rename_all = "camelCase")]
pub struct ReleaseIndexEntry {
    /// The release version (`2026.10.06-19`); never a dev version.
    #[cfg_attr(
        feature = "schema-gen",
        schemars(regex(pattern = r"^[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[1-9][0-9]{0,8}$"))
    )]
    pub version: String,
    /// The full commit, 40 lowercase hex.
    #[cfg_attr(feature = "schema-gen", schemars(regex(pattern = r"^[0-9a-f]{40}$")))]
    pub commit: String,
    /// The device wire protocol version the release speaks.
    pub wire_proto: u32,
    /// What a board must have to take the release without USB.
    pub requires: Requires,
    /// When the release was published (RFC 3339, UTC), as GitHub reports
    /// it. For display only: the version, not this, is the release's order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

#[derive(Deserialize)]
struct FormatProbe {
    format: Option<serde_json::Value>,
}

impl ReleaseIndex {
    /// An index of `target` holding `entries`, put newest first. Two entries
    /// for one version keep the first. Does not
    /// [`validate`](Self::validate).
    pub fn newest_first(target: &TargetName, mut entries: Vec<ReleaseIndexEntry>) -> Self {
        entries.sort_by(|a, b| {
            ReleaseVersion::parse(&b.version).cmp(&ReleaseVersion::parse(&a.version))
        });
        entries.dedup_by(|later, earlier| later.version == earlier.version);
        Self {
            format: RELEASE_INDEX_FORMAT,
            target: String::from(target.as_str()),
            releases: entries,
        }
    }

    /// Read release index bytes: the format first (another one is refused
    /// before its shape is looked at), then the format-1 shape. Unknown
    /// fields are ignored. Does **not** [`validate`](Self::validate).
    pub fn parse(bytes: &[u8]) -> Result<Self, ReleaseIndexError> {
        let probe: FormatProbe =
            serde_json::from_slice(bytes).map_err(|e| ReleaseIndexError::Json(format!("{e}")))?;
        let format = probe
            .format
            .as_ref()
            .and_then(serde_json::Value::as_u64)
            .ok_or(ReleaseIndexError::MissingFormat)?;
        if format != u64::from(RELEASE_INDEX_FORMAT) {
            return Err(ReleaseIndexError::UnsupportedFormat(format));
        }
        serde_json::from_slice(bytes).map_err(|e| ReleaseIndexError::Json(format!("{e}")))
    }

    /// Parse, then [`validate`](Self::validate).
    pub fn parse_valid(bytes: &[u8]) -> Result<Self, ReleaseIndexError> {
        let index = Self::parse(bytes)?;
        index.validate()?;
        Ok(index)
    }

    /// The bytes as served: pretty JSON in field order, with a trailing
    /// newline.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec_pretty(self).expect("a ReleaseIndex always serializes");
        out.push(b'\n');
        out
    }

    /// Check the structure: the target's grammar; every entry's version a
    /// release version and its commit 40 lowercase hex; versions unique and
    /// **strictly newest first** by the numeric order.
    pub fn validate(&self) -> Result<(), ReleaseIndexError> {
        if self.format != RELEASE_INDEX_FORMAT {
            return Err(ReleaseIndexError::UnsupportedFormat(u64::from(self.format)));
        }
        if !is_target_name(&self.target) {
            return Err(ReleaseIndexError::BadTarget(self.target.clone()));
        }
        let mut previous: Option<ReleaseVersion> = None;
        for entry in &self.releases {
            let version = ReleaseVersion::parse(&entry.version)
                .ok_or_else(|| ReleaseIndexError::BadVersion(entry.version.clone()))?;
            if !is_lower_hex(&entry.commit, COMMIT_HEX_LEN) {
                return Err(ReleaseIndexError::BadCommit(entry.commit.clone()));
            }
            if let Some(previous) = &previous {
                if version == *previous {
                    return Err(ReleaseIndexError::DuplicateVersion(entry.version.clone()));
                }
                if version > *previous {
                    return Err(ReleaseIndexError::NotNewestFirst {
                        older: previous.to_string(),
                        newer: entry.version.clone(),
                    });
                }
            }
            previous = Some(version);
        }
        Ok(())
    }

    /// The entry for `version`, if listed.
    pub fn entry(&self, version: &str) -> Option<&ReleaseIndexEntry> {
        self.releases.iter().find(|e| e.version == version)
    }
}

impl ReleaseIndexEntry {
    /// The entry for a release, from its `ota-manifest.json`: `version`,
    /// `commit`, `wireProto` and `requires` copied as they are, and when it
    /// was published. The one way an entry is built from a release.
    pub fn from_manifest(manifest: &OtaManifest, published_at: Option<String>) -> Self {
        Self {
            version: manifest.version.clone(),
            commit: manifest.commit.clone(),
            wire_proto: manifest.wire_proto,
            requires: manifest.requires,
            published_at,
        }
    }

    /// The build id: `version + "+" + commit[..12]`, as
    /// [`OtaManifest::build_id`]. Not a field.
    pub fn build_id(&self) -> String {
        let prefix = self
            .commit
            .get(..BUILD_ID_COMMIT_DIGITS)
            .unwrap_or(&self.commit);
        format!("{}+{}", self.version, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ota_manifest::tests::sample as sample_manifest;
    use alloc::vec;

    #[test]
    fn newest_first_sorts_by_number_and_validates() {
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        let index = ReleaseIndex::newest_first(
            &target,
            vec![
                entry("2026.10.06-9"),
                entry("2026.10.06-10"),
                entry("2026.10.05-30"),
            ],
        );
        let versions: Vec<&str> = index.releases.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(versions, ["2026.10.06-10", "2026.10.06-9", "2026.10.05-30"]);
        index.validate().unwrap();
        let back = ReleaseIndex::parse_valid(&index.to_json_bytes()).unwrap();
        assert_eq!(back, index);
    }

    #[test]
    fn newest_first_keeps_one_entry_per_version() {
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        let mut again = entry("2026.10.06-9");
        again.wire_proto = 99;
        let index = ReleaseIndex::newest_first(&target, vec![entry("2026.10.06-9"), again]);
        assert_eq!(index.releases.len(), 1);
        assert_eq!(index.releases[0].wire_proto, 39);
    }

    #[test]
    fn validate_refuses_an_order_that_is_not_newest_first() {
        let mut index = index_of(&["2026.10.06-9", "2026.10.06-10"]);
        assert_eq!(
            index.validate(),
            Err(ReleaseIndexError::NotNewestFirst {
                older: "2026.10.06-9".into(),
                newer: "2026.10.06-10".into(),
            })
        );
        index.releases.reverse();
        index.validate().unwrap();
        assert_eq!(
            index_of(&["2026.10.06-9", "2026.10.06-9"]).validate(),
            Err(ReleaseIndexError::DuplicateVersion("2026.10.06-9".into()))
        );
    }

    #[test]
    fn validate_refuses_bad_identity() {
        let mut index = index_of(&["2026.10.06-9"]);
        index.releases[0].version = "abc1234".into();
        assert_eq!(
            index.validate(),
            Err(ReleaseIndexError::BadVersion("abc1234".into()))
        );
        let mut index = index_of(&["2026.10.06-9"]);
        index.releases[0].commit = index.releases[0].commit.to_uppercase();
        assert!(matches!(
            index.validate(),
            Err(ReleaseIndexError::BadCommit(_))
        ));
        let mut index = index_of(&["2026.10.06-9"]);
        index.target = "ESP32C6".into();
        assert!(matches!(
            index.validate(),
            Err(ReleaseIndexError::BadTarget(_))
        ));
        // An empty index is well formed (the server answers 404 instead).
        index_of(&[]).validate().unwrap();
    }

    #[test]
    fn an_entry_copies_the_manifest_and_derives_its_build_id() {
        let manifest = sample_manifest();
        let e = ReleaseIndexEntry::from_manifest(&manifest, Some("2026-10-05T12:00:00Z".into()));
        assert_eq!(e.version, manifest.version);
        assert_eq!(e.commit, manifest.commit);
        assert_eq!(e.wire_proto, manifest.wire_proto);
        assert_eq!(e.requires, manifest.requires);
        assert_eq!(e.build_id(), manifest.build_id());
        let json = serde_json::to_value(&e).unwrap();
        assert!(json.get("buildId").is_none(), "a derivation, not a field");
        assert_eq!(json["publishedAt"], "2026-10-05T12:00:00Z");
        let without = ReleaseIndexEntry::from_manifest(&manifest, None);
        assert!(
            serde_json::to_value(&without)
                .unwrap()
                .get("publishedAt")
                .is_none(),
            "an absent publishedAt is not written"
        );
    }

    #[test]
    fn other_formats_are_refused_before_their_shape() {
        assert_eq!(
            ReleaseIndex::parse(br#"{"format":2,"whatever":true}"#),
            Err(ReleaseIndexError::UnsupportedFormat(2))
        );
        assert_eq!(
            ReleaseIndex::parse(br#"{"target":"x"}"#),
            Err(ReleaseIndexError::MissingFormat)
        );
        assert!(matches!(
            ReleaseIndex::parse(b"not json"),
            Err(ReleaseIndexError::Json(_))
        ));
    }

    fn entry(version: &str) -> ReleaseIndexEntry {
        ReleaseIndexEntry {
            version: version.to_string(),
            commit: "736d72856d243fce519c9f461f369f59fcbf175a".into(),
            wire_proto: 39,
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            published_at: None,
        }
    }

    fn index_of(versions: &[&str]) -> ReleaseIndex {
        ReleaseIndex {
            format: RELEASE_INDEX_FORMAT,
            target: "esp32c6-4mb".into(),
            releases: versions.iter().map(|v| entry(v)).collect(),
        }
    }
}
