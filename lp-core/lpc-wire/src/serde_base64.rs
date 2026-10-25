//! Base64 serialization helpers for binary payloads on the wire.
//!
//! Serialization wraps the bytes in the [`BLOB_MARKER`] newtype (the trick
//! serde_json's `RawValue` uses): a serializer that knows the marker
//! (`ser-write-json`'s token hook) can take the raw bytes as a blob, and any
//! other serializer is transparent to the newtype and writes the base64
//! string, streamed through `collect_str` with no heap `String`. The JSON
//! text is exactly what `base64::STANDARD.encode` + `serialize_str` wrote.

use alloc::{string::String, vec::Vec};
use core::fmt;
use serde::{Deserializer, Serialize, Serializer};

/// The newtype-struct name marking raw bytes whose text form is base64.
///
/// Must equal `ser_write_json::ser::BLOB_MARKER` (checked in tests).
pub const BLOB_MARKER: &str = "$lp::blob";

/// Serialize `Vec<u8>` as a base64 string.
pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serialize_blob(bytes, serializer)
}

/// Deserialize a base64 string to `Vec<u8>`.
///
/// Decoded straight from the text the deserializer holds, never through a
/// `String` copy of it: a JSON request with an 8 KB blob is ~11 KB of base64,
/// and a board decoding it had to find a second block of that size before
/// the decoded bytes' own (PR B: it reset an emulated C6).
pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(Base64Visitor)
}

/// Decodes the string it is handed (borrowed when the deserializer can lend
/// it, as `serde_json::from_str` does for a string with no escapes), with no
/// copy of the text.
struct Base64Visitor;

impl<'de> serde::de::Visitor<'de> for Base64Visitor {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a base64 string")
    }

    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Vec<u8>, E> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(E::custom)
    }
}

/// Serialize `Option<Vec<u8>>` as base64 (`None` → JSON null).
pub fn serialize_option<S>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match bytes {
        Some(bytes) => serializer.serialize_some(&Blob(bytes)),
        None => serializer.serialize_none(),
    }
}

/// Deserialize base64 string or null to `Option<Vec<u8>>`.
pub fn deserialize_option<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    use base64::Engine;
    use serde::Deserialize;
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => base64::engine::general_purpose::STANDARD
            .decode(s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

/// Raw bytes behind the [`BLOB_MARKER`] newtype.
///
/// Human-readable serializers get the standard, padded base64 string through
/// `collect_str`; a serializer that is not human-readable gets the bytes
/// (that is how `ser-write-json` reads them to offer a blob token).
struct Base64Text<'a>(&'a [u8]);

impl Serialize for Base64Text<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.collect_str(&Base64Display(self.0))
        } else {
            serializer.serialize_bytes(self.0)
        }
    }
}

/// Serialize `bytes` as the blob-marked base64 string.
fn serialize_blob<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_newtype_struct(BLOB_MARKER, &Base64Text(bytes))
}

/// [`serialize_blob`] as a value, for `serialize_some`.
struct Blob<'a>(&'a [u8]);

impl Serialize for Blob<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_blob(self.0, serializer)
    }
}

/// Standard base64 (RFC 4648 alphabet, `=` padded), streamed into a
/// formatter a stack buffer at a time.
struct Base64Display<'a>(&'a [u8]);

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl fmt::Display for Base64Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 48 input bytes per flush → 64 output characters.
        const CHUNK: usize = 48;
        let mut out = [0u8; CHUNK / 3 * 4];
        for block in self.0.chunks(CHUNK) {
            let mut n = 0;
            for group in block.chunks(3) {
                let b0 = group[0];
                let b1 = group.get(1).copied().unwrap_or(0);
                let b2 = group.get(2).copied().unwrap_or(0);
                let v = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
                out[n] = ALPHABET[(v >> 18) as usize & 63];
                out[n + 1] = ALPHABET[(v >> 12) as usize & 63];
                out[n + 2] = if group.len() > 1 {
                    ALPHABET[(v >> 6) as usize & 63]
                } else {
                    b'='
                };
                out[n + 3] = if group.len() > 2 {
                    ALPHABET[v as usize & 63]
                } else {
                    b'='
                };
                n += 4;
            }
            // The alphabet and `=` are ASCII.
            f.write_str(core::str::from_utf8(&out[..n]).map_err(|_| fmt::Error)?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        WireChannelSampleFormat, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
        WireTextureFormat,
    };
    use crate::server::{FileChangeKind, FileChunk, FsRequest, FsResponse};
    use crate::{
        ControlProductProbeResult, OutputFrameEntry, OutputFrameProbeResult, ProjectReadProbeEvent,
        ProjectReadResourceEvent, RenderProductProbeResult, RevisionGateResult, WireVisualSpace,
    };
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_model::{
        AsLpPathBuf, ControlExtent, ControlProduct, NodeId, ResourceRef, Revision, RuntimeBufferId,
        VisualProduct,
    };

    /// Not UTF-8; its base64 carries `+`, `/` and one `=`.
    const BINARY: [u8; 5] = [0xfb, 0xef, 0xbe, 0xef, 0xff];
    /// UTF-8 text a file body writes as a plain string.
    const TEXT: &[u8] = b"void main() {}\n";

    /// `value` serializes to exactly `expected` on serde_json, and on
    /// ser-write-json when the device serializer is compiled in.
    fn assert_json<T: serde::Serialize>(value: &T, expected: &str) {
        assert_eq!(
            crate::json::to_string(value).unwrap(),
            expected,
            "serde_json"
        );
        #[cfg(feature = "ser-write-json")]
        {
            let mut out = Vec::new();
            ser_write_json::ser::to_writer(&mut out, value).unwrap();
            assert_eq!(
                core::str::from_utf8(&out).unwrap(),
                expected,
                "ser-write-json"
            );
            let mut erased = Vec::new();
            crate::ser_write_json_to(&mut erased, value).unwrap();
            assert_eq!(erased, out, "erased writer");
        }
    }

    #[test]
    fn base64_display_matches_the_base64_crate_at_every_length() {
        use base64::Engine;
        let data: Vec<u8> = (0..=255u8).cycle().take(301).collect();
        for n in 0..=data.len() {
            let ours = Base64Display(&data[..n]).to_string();
            let theirs = base64::engine::general_purpose::STANDARD.encode(&data[..n]);
            assert_eq!(ours, theirs, "length {n}");
        }
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn blob_marker_matches_the_serializers() {
        assert_eq!(BLOB_MARKER, ser_write_json::ser::BLOB_MARKER);
    }

    #[test]
    fn runtime_buffer_payload_bytes_event() {
        let event = ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(9)),
            offset: 16,
            bytes: BINARY.to_vec(),
        };
        assert_json(
            &event,
            r#"{"runtime_buffer_payload_bytes":{"ref":{"domain":"runtime_buffer","id":9},"offset":16,"bytes":"++++7/8="}}"#,
        );
    }

    #[test]
    fn probe_result_bytes_event() {
        let event = ProjectReadProbeEvent::ResultBytes {
            offset: 4,
            bytes: BINARY.to_vec(),
        };
        assert_json(
            &event,
            r#"{"result_bytes":{"offset":4,"bytes":"++++7/8="}}"#,
        );
        let empty = ProjectReadProbeEvent::ResultBytes {
            offset: 0,
            bytes: Vec::new(),
        };
        assert_json(&empty, r#"{"result_bytes":{"offset":0,"bytes":""}}"#);
    }

    #[test]
    fn render_product_texture() {
        let result = RenderProductProbeResult::Texture {
            product: VisualProduct::new(NodeId::new(3), 0),
            revision: Revision::new(5),
            width: 1,
            height: 1,
            format: WireTextureFormat::Srgb8,
            bytes: vec![1, 2, 3, 4],
            space: WireVisualSpace::TwoD,
            projection: None,
            origin: None,
            primary: WireVisualSpace::TwoD,
        };
        assert_json(
            &result,
            r#"{"texture":{"product":{"node":3,"output":0},"revision":5,"width":1,"height":1,"format":"srgb8","bytes":"AQIDBA==","space":"two_d","primary":"two_d"}}"#,
        );
    }

    #[test]
    fn output_frame_entry() {
        let result = OutputFrameProbeResult::Frame {
            outputs: vec![OutputFrameEntry {
                node: NodeId::new(4),
                revision: Revision::new(6),
                channels: 1,
                sample_format: Some(WireChannelSampleFormat::U16),
                geometry: RevisionGateResult::Unchanged {
                    revision: Revision::new(6),
                },
                bytes: vec![0xff, 0x00, 0x7f, 0x80, 0x01, 0xfe],
            }],
        };
        assert_json(
            &result,
            r#"{"frame":{"outputs":[{"node":4,"revision":6,"channels":1,"sample_format":"u16","geometry":{"unchanged":{"revision":6}},"bytes":"/wB/gAH+"}]}}"#,
        );
    }

    #[test]
    fn control_product_preview() {
        let product = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 3));
        let result = ControlProductProbeResult::Preview {
            product,
            revision: Revision::new(18),
            extent: ControlExtent::new(1, 3),
            sample_format: WireChannelSampleFormat::U16,
            geometry: RevisionGateResult::Unchanged {
                revision: Revision::new(18),
            },
            bytes: BINARY.to_vec(),
        };
        assert_json(
            &result,
            r#"{"preview":{"product":{"node":2,"output":0,"preferred_extent":{"rows":1,"samples_per_row":3}},"revision":18,"extent":{"rows":1,"samples_per_row":3},"sample_format":"u16","geometry":{"unchanged":{"revision":18}},"bytes":"++++7/8="}}"#,
        );
    }

    #[test]
    fn runtime_buffer_payload() {
        let payload = WireRuntimeBufferPayload {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(3)),
            revision: Revision::new(2),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: vec![1, 2, 3],
        };
        assert_json(
            &payload,
            r#"{"ref":{"domain":"runtime_buffer","id":3},"revision":2,"metadata":"raw","bytes":"AQID"}"#,
        );
    }

    #[test]
    fn file_chunk_bodies_are_text_or_a_base64_object() {
        let text = FileChunk {
            path: "/shader.glsl".as_path_buf(),
            kind: FileChangeKind::Upsert,
            offset: 0,
            total: TEXT.len() as u32,
            data: TEXT.to_vec(),
        };
        assert_json(
            &text,
            r#"{"path":"/shader.glsl","kind":"upsert","offset":0,"total":15,"data":"void main() {}\n"}"#,
        );
        let binary = FileChunk {
            path: "/blob.bin".as_path_buf(),
            kind: FileChangeKind::Upsert,
            offset: 8,
            total: 13,
            data: BINARY.to_vec(),
        };
        assert_json(
            &binary,
            r#"{"path":"/blob.bin","kind":"upsert","offset":8,"total":13,"data":{"base64":"++++7/8="}}"#,
        );
    }

    #[test]
    fn fs_api_file_bodies_are_text_or_a_base64_object() {
        let write = FsRequest::Write {
            path: "/a.bin".as_path_buf(),
            data: BINARY.to_vec(),
        };
        assert_json(
            &write,
            r#"{"write":{"path":"/a.bin","data":{"base64":"++++7/8="}}}"#,
        );
        let write_text = FsRequest::Write {
            path: "/a.glsl".as_path_buf(),
            data: TEXT.to_vec(),
        };
        assert_json(
            &write_text,
            r#"{"write":{"path":"/a.glsl","data":"void main() {}\n"}}"#,
        );
        let chunk = FsRequest::WriteChunk {
            path: "/a.bin".as_path_buf(),
            offset: 5,
            data: BINARY.to_vec(),
        };
        assert_json(
            &chunk,
            r#"{"writeChunk":{"path":"/a.bin","offset":5,"data":{"base64":"++++7/8="}}}"#,
        );
        let read = FsResponse::Read {
            path: "/a.bin".as_path_buf(),
            data: Some(BINARY.to_vec()),
            error: None,
        };
        assert_json(
            &read,
            r#"{"read":{"path":"/a.bin","data":{"base64":"++++7/8="},"error":null}}"#,
        );
        let read_text = FsResponse::Read {
            path: "/a.glsl".as_path_buf(),
            data: Some(TEXT.to_vec()),
            error: None,
        };
        assert_json(
            &read_text,
            r#"{"read":{"path":"/a.glsl","data":"void main() {}\n","error":null}}"#,
        );
        let missing = FsResponse::Read {
            path: "/gone".as_path_buf(),
            data: None,
            error: Some("not found".to_string()),
        };
        assert_json(
            &missing,
            r#"{"read":{"path":"/gone","data":null,"error":"not found"}}"#,
        );
    }

    /// Every message that carries a file body, with `bytes` as the body:
    /// write, chunked write, a read's answer, a changes page's chunk.
    fn file_messages(bytes: &[u8]) -> [(&'static str, String); 4] {
        let path = || "/f".as_path_buf();
        [
            (
                "Write",
                crate::json::to_string(&FsRequest::Write {
                    path: path(),
                    data: bytes.to_vec(),
                })
                .unwrap(),
            ),
            (
                "WriteChunk",
                crate::json::to_string(&FsRequest::WriteChunk {
                    path: path(),
                    offset: 0,
                    data: bytes.to_vec(),
                })
                .unwrap(),
            ),
            (
                "Read",
                crate::json::to_string(&FsResponse::Read {
                    path: path(),
                    data: Some(bytes.to_vec()),
                    error: None,
                })
                .unwrap(),
            ),
            (
                "FileChunk",
                crate::json::to_string(&FileChunk {
                    path: path(),
                    kind: FileChangeKind::Upsert,
                    offset: 0,
                    total: bytes.len() as u32,
                    data: bytes.to_vec(),
                })
                .unwrap(),
            ),
        ]
    }

    /// The body `json` (one of [`file_messages`]) carries, read by `name`'s
    /// type from a `&str` and from a `&[u8]`, which must agree.
    fn body_read_back(name: &str, json: &str) -> Vec<u8> {
        fn both<T: serde::de::DeserializeOwned>(
            json: &str,
            data: impl Fn(T) -> Vec<u8>,
        ) -> Vec<u8> {
            let from_str = data(crate::json::from_str::<T>(json).unwrap());
            let from_slice = data(crate::json::from_slice::<T>(json.as_bytes()).unwrap());
            assert_eq!(from_str, from_slice, "{json}");
            from_str
        }
        match name {
            "Write" => both::<FsRequest>(json, |request| match request {
                FsRequest::Write { data, .. } => data,
                other => panic!("{other:?}"),
            }),
            "WriteChunk" => both::<FsRequest>(json, |request| match request {
                FsRequest::WriteChunk { data, .. } => data,
                other => panic!("{other:?}"),
            }),
            "Read" => both::<FsResponse>(json, |response| match response {
                FsResponse::Read { data, .. } => data.expect("a body"),
                other => panic!("{other:?}"),
            }),
            "FileChunk" => both::<FileChunk>(json, |chunk| chunk.data),
            other => panic!("{other}"),
        }
    }

    /// The ticket's defect: text that happens to spell base64 (`abcd` is
    /// valid base64 of three non-UTF-8 bytes) used to come back as those
    /// bytes. A string is now always the text itself.
    #[test]
    fn text_that_spells_base64_stays_text_in_every_file_message() {
        for text in ["abcd", "1234", "++++7/8=", "AQID", "////", "QUJD"] {
            for (name, json) in file_messages(text.as_bytes()) {
                assert!(
                    json.contains(&alloc::format!(r#""data":"{text}""#)),
                    "{name}: {json}"
                );
                assert_eq!(body_read_back(name, &json), text.as_bytes(), "{name}");
            }
        }
    }

    #[test]
    fn binary_and_empty_bodies_round_trip_in_every_file_message() {
        let every_byte: Vec<u8> = (0..=255u8).collect();
        for body in [
            &BINARY[..],
            &[0xff][..],
            &every_byte[..],
            &[0u8, 159, 146, 150][..],
        ] {
            for (name, json) in file_messages(body) {
                assert!(json.contains(r#""data":{"base64":""#), "{name}: {json}");
                assert_eq!(body_read_back(name, &json), body, "{name}");
            }
        }
        for (name, json) in file_messages(b"") {
            assert!(json.contains(r#""data":"""#), "{name}: {json}");
            assert_eq!(body_read_back(name, &json), b"", "{name}");
        }
    }

    /// The old ambiguous form is gone, not tolerated: a bare base64 string
    /// is text, and an array of numbers is no body.
    #[test]
    fn a_bare_base64_string_is_text_not_binary() {
        let json = r#"{"write":{"path":"/a.bin","data":"++++7/8="}}"#;
        assert_eq!(body_read_back("Write", json), b"++++7/8=");
        assert!(
            crate::json::from_str::<FsRequest>(r#"{"write":{"path":"/a","data":[1,2]}}"#).is_err()
        );
    }

    #[test]
    fn option_helpers() {
        #[derive(serde::Serialize)]
        struct Opt {
            #[serde(serialize_with = "serialize_option")]
            some: Option<Vec<u8>>,
            #[serde(serialize_with = "serialize_option")]
            none: Option<Vec<u8>>,
        }
        let value = Opt {
            some: Some(BINARY.to_vec()),
            none: None,
        };
        assert_json(&value, r#"{"some":"++++7/8=","none":null}"#);
    }
}
