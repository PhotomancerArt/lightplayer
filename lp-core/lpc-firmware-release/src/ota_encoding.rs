//! `encodings[]`: compressed copies of the pieces, chosen by `id` alone.
//!
//! **Readers choose an encoding by `id` alone** (doors #3); the other keys
//! describe it for people and tools. **An entry whose `id` this reader does
//! not know is skipped, never an error**, even when its other keys have
//! shapes this reader cannot parse: a new encoding is appended to the list
//! and keeps `format: 1`. So each entry is parsed leniently — only `id` is
//! required — and the known ids are decoded on demand.
//!
//! `id: 1` ([`Encoding1`]) is M4's dictionary rule (`lpc-update`): core
//! base 0, engine base 4 KiB, a 32 KiB window, no dictionary for the
//! engine's header sector, and an independent raw-deflate stream per 4 KiB
//! chunk. **This crate holds no copy of that rule**: it describes the files
//! and checks their structure; whether a chunk decodes is proven by the
//! producer (`lp-cli firmware release-check`, with M4's prover).

use alloc::string::String;
// The schemars `regex` attribute expands to `.to_string()`.
#[cfg(feature = "schema-gen")]
use alloc::string::ToString;
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// The encoding id M4's dictionary rule answers to (and bit 0 of the update
/// protocol's request flags).
pub const ENCODING_DEFLATE_DICT_V1: u32 = 1;

/// Encoding 1: a `.z` file per piece, its chunks back to back, indexed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Encoding1 {
    /// Always `1`.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1, max = 1)))]
    pub id: u32,
    /// For people and tools: `"deflate-raw"` (RFC 1951, preset dictionary).
    pub codec: String,
    /// Raw piece bytes per chunk: `4096` (the last chunk is short).
    pub chunk_bytes: u32,
    /// The dictionary window: `32768`.
    pub window_bytes: u32,
    /// The core's compressed chunks.
    pub core: EncodedPieceFile,
    /// The engine's compressed chunks.
    pub engine: EncodedPieceFile,
}

/// One piece's compressed file: its chunks back to back, with no header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EncodedPieceFile {
    /// The file's name inside the release (`core.z`).
    pub file: String,
    /// The file's length in bytes: the sum of `chunks`.
    pub length: u64,
    /// SHA-256 of the file, lowercase hex.
    #[cfg_attr(feature = "schema-gen", schemars(regex(pattern = r"^[0-9a-f]{64}$")))]
    pub sha256: String,
    /// `chunks[i]` = the compressed length of raw chunk `i`; **`0` means "no
    /// compressed form, send raw"**. One entry per `ceil(piece length /
    /// chunkBytes)` chunk; offsets are the running sum.
    pub chunks: Vec<u32>,
}

impl EncodedPieceFile {
    /// Byte range of chunk `i` inside the `.z` file, or `None` when chunk `i`
    /// has no compressed form (`0`) or does not exist.
    pub fn chunk_range(&self, i: usize) -> Option<core::ops::Range<u64>> {
        let len = u64::from(*self.chunks.get(i)?);
        if len == 0 {
            return None;
        }
        let start: u64 = self.chunks[..i].iter().map(|c| u64::from(*c)).sum();
        Some(start..start + len)
    }
}

/// One entry of `encodings[]`: a known encoding, decoded, or an opaque one
/// this reader skips.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodingEntry {
    id: u32,
    body: EncodingBody,
}

#[derive(Debug, Clone, PartialEq)]
enum EncodingBody {
    Deflate1(Encoding1),
    /// An id this reader does not know, or an id-1 entry that does not
    /// parse (which `validate()` refuses). Kept as written.
    Opaque(Value),
}

impl EncodingEntry {
    /// Wrap an encoding-1 description (the producer's constructor).
    pub fn deflate1(encoding: Encoding1) -> Self {
        Self {
            id: ENCODING_DEFLATE_DICT_V1,
            body: EncodingBody::Deflate1(encoding),
        }
    }

    /// The entry's id.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Encoding 1, when this entry is one this reader could decode.
    pub fn as_encoding1(&self) -> Option<&Encoding1> {
        match &self.body {
            EncodingBody::Deflate1(e) => Some(e),
            EncodingBody::Opaque(_) => None,
        }
    }

    /// True when this reader knows the id but could not decode the entry.
    pub(crate) fn is_malformed_known(&self) -> bool {
        self.id == ENCODING_DEFLATE_DICT_V1 && matches!(self.body, EncodingBody::Opaque(_))
    }
}

impl<'de> Deserialize<'de> for EncodingEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let id = value
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .ok_or_else(|| serde::de::Error::custom("an encoding entry needs an integer `id`"))?;
        let body = if id == ENCODING_DEFLATE_DICT_V1 {
            match serde_json::from_value::<Encoding1>(value.clone()) {
                Ok(e) => EncodingBody::Deflate1(e),
                Err(_) => EncodingBody::Opaque(value),
            }
        } else {
            EncodingBody::Opaque(value)
        };
        Ok(Self { id, body })
    }
}

impl Serialize for EncodingEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.body {
            EncodingBody::Deflate1(e) => e.serialize(serializer),
            EncodingBody::Opaque(v) => v.serialize(serializer),
        }
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for EncodingEntry {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "EncodingEntry".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let encoding1 = generator.subschema_for::<Encoding1>();
        schemars::json_schema!({
            "description": "One compressed copy of the pieces. Readers choose by `id` alone and skip an id they do not know, whatever its other keys; only `id` is required. `id: 1` is the update protocol's dictionary rule (lpc-update) and must have Encoding1's shape.",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "integer", "minimum": 0, "maximum": u32::MAX }
            },
            "if": { "properties": { "id": { "const": 1 } } },
            "then": encoding1
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn piece(chunks: Vec<u32>) -> EncodedPieceFile {
        EncodedPieceFile {
            file: "core.z".into(),
            length: chunks.iter().map(|c| u64::from(*c)).sum(),
            sha256: "0".repeat(64),
            chunks,
        }
    }

    #[test]
    fn chunk_ranges_skip_raw_chunks() {
        let p = piece(vec![10, 0, 7]);
        assert_eq!(p.chunk_range(0), Some(0..10));
        assert_eq!(p.chunk_range(1), None);
        assert_eq!(p.chunk_range(2), Some(10..17));
        assert_eq!(p.chunk_range(3), None);
    }

    #[test]
    fn unknown_ids_parse_as_opaque_and_write_back() {
        let json = r#"{"id":7,"codec":{"weird":[1,2]},"chunks":"not a list"}"#;
        let entry: EncodingEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id(), 7);
        assert!(entry.as_encoding1().is_none());
        assert!(!entry.is_malformed_known());
        let back: Value = serde_json::to_value(&entry).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(json).unwrap());
    }

    #[test]
    fn a_malformed_id_1_is_kept_and_flagged() {
        let entry: EncodingEntry =
            serde_json::from_str(r#"{"id":1,"codec":"deflate-raw"}"#).unwrap();
        assert!(entry.as_encoding1().is_none());
        assert!(entry.is_malformed_known());
    }

    #[test]
    fn an_entry_without_an_id_is_refused() {
        assert!(serde_json::from_str::<EncodingEntry>(r#"{"codec":"x"}"#).is_err());
        assert!(serde_json::from_str::<EncodingEntry>(r#"{"id":"1"}"#).is_err());
    }
}
