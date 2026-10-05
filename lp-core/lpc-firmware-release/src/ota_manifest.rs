//! `ota-manifest.json` format 1: what one release holds for one target.

use alloc::format;
use alloc::string::String;
// The schemars `regex` attribute expands to `.to_string()`.
#[cfg(feature = "schema-gen")]
use alloc::string::ToString;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::dev_version::is_app_version;
use crate::firmware_lookup_path::{OTA_MANIFEST_FILE, is_lookup_file_name};
use crate::lower_hex::{BUILD_ID_COMMIT_DIGITS, COMMIT_HEX_LEN, SHA256_HEX_LEN, is_lower_hex};
use crate::ota_encoding::{ENCODING_DEFLATE_DICT_V1, EncodedPieceFile, Encoding1, EncodingEntry};
use crate::ota_manifest_error::OtaManifestError;
use crate::release_version::is_release_version;
use crate::target_name::is_target_name;

/// The only `format` this crate reads and writes.
pub const OTA_MANIFEST_FORMAT: u32 = 1;

/// `ota-manifest.json`, **format 1**: one target's build in one release —
/// its identity, what a board must have to take it, its two pieces, their
/// compressed encodings, and the USB package it came from.
///
/// One JSON object, camelCase, lowercase 64-hex SHA-256s, byte lengths as
/// integers, **no timestamp** (the manifest is a function of the bytes it
/// describes). Published as the release asset `<target>.ota-manifest.json`
/// and served at `/firmware/<target>/<release>/ota-manifest.json`. Its
/// schema is `schemas/ota-manifest.schema.json`.
///
/// # Compatibility (an archive door: every release carries one, forever)
///
/// - `format` is `1`. Readers **refuse** another `format` and **ignore
///   unknown fields** (no `deny_unknown_fields`).
/// - An additive optional field keeps `format: 1`, and so does a new
///   encoding (appended to `encodings`; unknown ids are skipped).
/// - `format` is bumped only when an old reader would misread something, and
///   then the old form is still written beside the new one until no
///   supported Studio reads it.
/// - `version`, `commit` and `target` are never re-rendered: a different
///   spelling of the same value is a format change (the 2026-08-07 uid
///   incident, AGENTS.md "Persisted-format compatibility").
///
/// # The board reports exactly this identity (doors #2)
///
/// A board running this release says, in its board manifest:
///
/// | Board manifest | here | rule |
/// |---|---|---|
/// | `target`, `chip`, `version`, `wireProto` | same keys | equal |
/// | `buildId` | [`OtaManifest::build_id`] | equal |
/// | `coreSha256`, `coreLen` | `core.sha256`, `core.length` | equal |
/// | `engineSha256`, `engineLen` | `engine.sha256`, `engine.length` | equal (also the core's digest slot and Studio's engine-cache key) |
/// | `layout` | `requires.layout` | an install needs them **equal** |
/// | `loader` | `requires.loader` | an install needs the board's **≥** this |
///
/// The board is the compatibility authority; `requires` only *predicts*
/// "this needs USB once". The update protocol (M4) tests the equality on the
/// emulator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema-gen",
    schemars(
        title = "ota-manifest.json",
        description = "The OTA release manifest, format 1 (lpc_firmware_release::OtaManifest). Readers refuse another format and ignore unknown fields and unknown encoding ids; an additive field or a new encoding keeps format 1. See the firmware-distribution ADR."
    )
)]
#[serde(rename_all = "camelCase")]
pub struct OtaManifest {
    /// Shape version; always `1` ([`OTA_MANIFEST_FORMAT`]).
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1, max = 1)))]
    pub format: u32,
    /// The target (a line of builds, e.g. `esp32c6-4mb`); opaque.
    #[cfg_attr(
        feature = "schema-gen",
        schemars(regex(pattern = r"^[a-z0-9][a-z0-9-]{0,63}$"))
    )]
    pub target: String,
    /// The chip word, as the build def's `chip.name` and the board say it.
    pub chip: String,
    /// The app version: a release (`2026.10.05-3`) or, in a local package
    /// only, a dev version (`abc1234[-dirty-HHMMSSPT]`).
    pub version: String,
    /// The full commit, 40 lowercase hex.
    #[cfg_attr(feature = "schema-gen", schemars(regex(pattern = r"^[0-9a-f]{40}$")))]
    pub commit: String,
    /// The device wire protocol version, from the image's manifest core.
    pub wire_proto: u32,
    /// What a board must have to take this build without USB.
    pub requires: Requires,
    /// `core.bin`.
    pub core: PieceFile,
    /// `engine.bin`, exactly as flashed (header committed).
    pub engine: PieceFile,
    /// Compressed copies, chosen by `id`; possibly empty (raw only).
    #[serde(default)]
    pub encodings: Vec<EncodingEntry>,
    /// The USB package (schemaVersion 2 `package.json`) and its merged image.
    pub package: PackageRef,
}

/// What a board must have to take a build over the air. Only a prediction:
/// the board decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Requires {
    /// The flash layout; the board's must be **equal** (M4's code table:
    /// `1` = layout 1).
    pub layout: u16,
    /// The loader version; the board's must be **≥** this (M4's code table:
    /// `1` = M2's loader, `0` = no version word).
    pub loader: u16,
}

/// A file the manifest names: its name in the lookup, length, SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PieceFile {
    /// The file's name inside the release (`core.bin`).
    pub file: String,
    /// Length in bytes.
    pub length: u64,
    /// SHA-256, lowercase hex.
    #[cfg_attr(feature = "schema-gen", schemars(regex(pattern = r"^[0-9a-f]{64}$")))]
    pub sha256: String,
}

/// The USB package this build came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PackageRef {
    /// The package manifest's name inside the lookup (`package.json`).
    pub file: String,
    /// Length in bytes.
    pub length: u64,
    /// SHA-256, lowercase hex.
    #[cfg_attr(feature = "schema-gen", schemars(regex(pattern = r"^[0-9a-f]{64}$")))]
    pub sha256: String,
    /// The merged image the package flashes.
    pub image: PieceFile,
}

#[derive(Deserialize)]
struct FormatProbe {
    format: Option<serde_json::Value>,
}

impl OtaManifest {
    /// Read `ota-manifest.json` bytes: the format first (another one is
    /// refused before its shape is looked at), then the format-1 shape.
    /// Unknown fields and unknown encoding ids are ignored. Does **not**
    /// [`validate`](Self::validate).
    pub fn parse(bytes: &[u8]) -> Result<Self, OtaManifestError> {
        let probe: FormatProbe =
            serde_json::from_slice(bytes).map_err(|e| OtaManifestError::Json(format!("{e}")))?;
        let format = probe
            .format
            .as_ref()
            .and_then(serde_json::Value::as_u64)
            .ok_or(OtaManifestError::MissingFormat)?;
        if format != u64::from(OTA_MANIFEST_FORMAT) {
            return Err(OtaManifestError::UnsupportedFormat(format));
        }
        serde_json::from_slice(bytes).map_err(|e| OtaManifestError::Json(format!("{e}")))
    }

    /// Parse, then [`validate`](Self::validate).
    pub fn parse_valid(bytes: &[u8]) -> Result<Self, OtaManifestError> {
        let manifest = Self::parse(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The bytes as published: pretty JSON in field order, with a trailing
    /// newline.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec_pretty(self).expect("an OtaManifest always serializes");
        out.push(b'\n');
        out
    }

    /// The build id: `version + "+" + commit[..12]` (N1). Not a field: two
    /// fields that could disagree are one derivation.
    pub fn build_id(&self) -> String {
        let prefix = self
            .commit
            .get(..BUILD_ID_COMMIT_DIGITS)
            .unwrap_or(&self.commit);
        format!("{}+{}", self.version, prefix)
    }

    /// True when `version` is a release version (only those are ever in the
    /// store); false for a local package's dev version.
    pub fn is_release(&self) -> bool {
        is_release_version(&self.version)
    }

    /// The first encoding-1 entry this reader could decode, if any.
    pub fn encoding1(&self) -> Option<&Encoding1> {
        self.encodings.iter().find_map(EncodingEntry::as_encoding1)
    }

    /// The entry with this id, if present (decoded or not).
    pub fn encoding(&self, id: u32) -> Option<&EncodingEntry> {
        self.encodings.iter().find(|e| e.id() == id)
    }

    /// Check the structure: the grammar of every name, hex and version; file
    /// names unique and in the lookup grammar; and for each **known**
    /// encoding, the chunk count and that `chunks` sums to the `.z` length.
    /// Does not decode a chunk (the producer proves that with M4's prover).
    pub fn validate(&self) -> Result<(), OtaManifestError> {
        if self.format != OTA_MANIFEST_FORMAT {
            return Err(OtaManifestError::UnsupportedFormat(u64::from(self.format)));
        }
        if !is_target_name(&self.target) {
            return Err(OtaManifestError::BadTarget(self.target.clone()));
        }
        if !is_target_name(&self.chip) {
            return Err(OtaManifestError::BadChip(self.chip.clone()));
        }
        if !is_app_version(&self.version) {
            return Err(OtaManifestError::BadVersion(self.version.clone()));
        }
        if !is_lower_hex(&self.commit, COMMIT_HEX_LEN) {
            return Err(OtaManifestError::BadCommit(self.commit.clone()));
        }
        check_sha("core.sha256", &self.core.sha256)?;
        check_sha("engine.sha256", &self.engine.sha256)?;
        check_sha("package.sha256", &self.package.sha256)?;
        check_sha("package.image.sha256", &self.package.image.sha256)?;
        for entry in &self.encodings {
            if entry.is_malformed_known() {
                return Err(OtaManifestError::MalformedEncoding { id: entry.id() });
            }
            if let Some(e) = entry.as_encoding1() {
                if e.chunk_bytes == 0 {
                    return Err(OtaManifestError::ZeroChunkBytes {
                        id: ENCODING_DEFLATE_DICT_V1,
                    });
                }
                check_sha("encodings[1].core.sha256", &e.core.sha256)?;
                check_sha("encodings[1].engine.sha256", &e.engine.sha256)?;
                check_chunks(&e.core, self.core.length, e.chunk_bytes)?;
                check_chunks(&e.engine, self.engine.length, e.chunk_bytes)?;
            }
        }
        let names: Vec<&str> = self.files().iter().map(|f| f.file).collect();
        for (i, name) in names.iter().enumerate() {
            if !is_lookup_file_name(name) || *name == OTA_MANIFEST_FILE {
                return Err(OtaManifestError::BadFileName(String::from(*name)));
            }
            if names[..i].contains(name) {
                return Err(OtaManifestError::DuplicateFile(String::from(*name)));
            }
        }
        Ok(())
    }
}

fn check_sha(field: &'static str, sha: &str) -> Result<(), OtaManifestError> {
    if is_lower_hex(sha, SHA256_HEX_LEN) {
        Ok(())
    } else {
        Err(OtaManifestError::BadSha256 { field })
    }
}

fn check_chunks(
    z: &EncodedPieceFile,
    piece_length: u64,
    chunk_bytes: u32,
) -> Result<(), OtaManifestError> {
    let expected = piece_length.div_ceil(u64::from(chunk_bytes));
    let actual = z.chunks.len() as u64;
    if actual != expected {
        return Err(OtaManifestError::ChunkCount {
            file: z.file.clone(),
            expected,
            actual,
        });
    }
    let sum: u64 = z.chunks.iter().map(|c| u64::from(*c)).sum();
    if sum != z.length {
        return Err(OtaManifestError::ChunkSum {
            file: z.file.clone(),
            expected: z.length,
            actual: sum,
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    pub(crate) fn sample() -> OtaManifest {
        let sha = |c: char| c.to_string().repeat(64);
        OtaManifest {
            format: 1,
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.05-3".into(),
            commit: "abc1234567890123456789012345678901234567".into(),
            wire_proto: 36,
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            core: PieceFile {
                file: "core.bin".into(),
                length: 10_000,
                sha256: sha('1'),
            },
            engine: PieceFile {
                file: "engine.bin".into(),
                length: 9_000,
                sha256: sha('2'),
            },
            encodings: vec![EncodingEntry::deflate1(Encoding1 {
                id: 1,
                codec: "deflate-raw".into(),
                chunk_bytes: 4096,
                window_bytes: 32768,
                core: EncodedPieceFile {
                    file: "core.z".into(),
                    length: 600,
                    sha256: sha('3'),
                    chunks: vec![300, 0, 300],
                },
                engine: EncodedPieceFile {
                    file: "engine.z".into(),
                    length: 500,
                    sha256: sha('4'),
                    chunks: vec![0, 250, 250],
                },
            })],
            package: PackageRef {
                file: "package.json".into(),
                length: 100,
                sha256: sha('5'),
                image: PieceFile {
                    file: "fw-esp32c6-merged.bin".into(),
                    length: 4_194_304,
                    sha256: sha('6'),
                },
            },
        }
    }

    #[test]
    fn the_sample_validates_and_round_trips() {
        let m = sample();
        m.validate().unwrap();
        let back = OtaManifest::parse_valid(&m.to_json_bytes()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn build_id_is_version_plus_twelve_commit_digits() {
        assert_eq!(sample().build_id(), "2026.10.05-3+abc123456789");
        assert!(sample().is_release());
        let mut dev = sample();
        dev.version = "abc1234-dirty-101500PT".into();
        dev.validate().unwrap();
        assert!(!dev.is_release());
        assert_eq!(dev.build_id(), "abc1234-dirty-101500PT+abc123456789");
    }

    #[test]
    fn validate_refuses_bad_identity() {
        let mut m = sample();
        m.commit = "abc123456789".into();
        assert!(matches!(m.validate(), Err(OtaManifestError::BadCommit(_))));
        let mut m = sample();
        m.commit = m.commit.to_uppercase();
        assert!(matches!(m.validate(), Err(OtaManifestError::BadCommit(_))));
        let mut m = sample();
        m.target = "ESP32C6".into();
        assert!(matches!(m.validate(), Err(OtaManifestError::BadTarget(_))));
        let mut m = sample();
        m.version = "v2026.10.05-3".into();
        assert!(matches!(m.validate(), Err(OtaManifestError::BadVersion(_))));
        let mut m = sample();
        m.engine.sha256 = "A".repeat(64);
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::BadSha256 {
                field: "engine.sha256"
            })
        );
        let mut m = sample();
        m.core.sha256 = "1".repeat(63);
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::BadSha256 {
                field: "core.sha256"
            })
        );
    }

    #[test]
    fn validate_refuses_bad_chunk_indexes() {
        let mut m = sample();
        let mut e = m.encoding1().unwrap().clone();
        e.core.chunks = vec![300, 300];
        e.core.length = 600;
        m.encodings = vec![EncodingEntry::deflate1(e)];
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::ChunkCount {
                file: "core.z".into(),
                expected: 3,
                actual: 2
            })
        );

        let mut m = sample();
        let mut e = m.encoding1().unwrap().clone();
        e.engine.length = 501;
        m.encodings = vec![EncodingEntry::deflate1(e)];
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::ChunkSum {
                file: "engine.z".into(),
                expected: 501,
                actual: 500
            })
        );

        let mut m = sample();
        let mut e = m.encoding1().unwrap().clone();
        e.chunk_bytes = 0;
        m.encodings = vec![EncodingEntry::deflate1(e)];
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::ZeroChunkBytes { id: 1 })
        );
    }

    #[test]
    fn validate_refuses_bad_or_duplicate_file_names() {
        let mut m = sample();
        m.engine.file = "core.bin".into();
        assert_eq!(
            m.validate(),
            Err(OtaManifestError::DuplicateFile("core.bin".into()))
        );
        let mut m = sample();
        m.package.file = "ota-manifest.json".into();
        assert!(matches!(
            m.validate(),
            Err(OtaManifestError::BadFileName(_))
        ));
        let mut m = sample();
        m.core.file = "../core.bin".into();
        assert!(matches!(
            m.validate(),
            Err(OtaManifestError::BadFileName(_))
        ));
    }

    #[test]
    fn missing_or_empty_encodings_are_raw_only() {
        let mut m = sample();
        m.encodings.clear();
        m.validate().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&m.to_json_bytes()).unwrap();
        value.as_object_mut().unwrap().remove("encodings");
        let parsed = OtaManifest::parse_valid(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(parsed.encodings.is_empty());
        assert!(parsed.encoding1().is_none());
    }

    #[test]
    fn other_formats_are_refused_before_their_shape() {
        assert_eq!(
            OtaManifest::parse(br#"{"format":2,"whatever":true}"#),
            Err(OtaManifestError::UnsupportedFormat(2))
        );
        assert_eq!(
            OtaManifest::parse(br#"{"target":"x"}"#),
            Err(OtaManifestError::MissingFormat)
        );
        assert_eq!(
            OtaManifest::parse(br#"{"format":"1"}"#),
            Err(OtaManifestError::MissingFormat)
        );
        assert!(matches!(
            OtaManifest::parse(b"not json"),
            Err(OtaManifestError::Json(_))
        ));
    }
}
