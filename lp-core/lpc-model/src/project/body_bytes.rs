//! A whole file body on the edit wire: UTF-8 text as a JSON string, any
//! other bytes as `{"base64":"…"}`.
//!
//! The bodies Studio sends a board — an overlay's
//! [`ReplaceBody`](crate::AssetBodyOverlay::ReplaceBody), a created node's
//! def and assets — are `Vec<u8>`, because a project may hold an image or a
//! binary asset. serde writes a `Vec<u8>` as a JSON array of numbers, ~3.5
//! characters a byte: a 1,971 B shader was a ~7.1 KB request, and past
//! ~4.7 KB of source an edit outgrew the board's message cap and was dropped
//! unanswered
//! (`docs/defects/2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md`).
//!
//! Every body the editors write is text, so text goes as itself — a shader
//! costs its own length plus its escapes (a newline is two characters).
//! Bytes that are not UTF-8 go as base64 inside an object, so the two forms
//! can never be read for each other: a JSON string is always the text's own
//! bytes, even one that happens to spell valid base64 (`"abcd"` is four
//! letters, never three binary bytes). Which form a body takes is decided by
//! its bytes alone, so each body has exactly one encoding.
//!
//! Use as `#[serde(with = "lpc_model::body_bytes")]` on a `Vec<u8>`, or
//! [`BodyRef`]/[`BodyBuf`] where the bytes sit inside another container.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use base64::Engine;
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The one key of a binary body's object.
pub const BASE64_KEY: &str = "base64";

/// Serialize a body: text as a JSON string, other bytes as
/// `{"base64":"…"}` (standard alphabet, padded).
pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    match core::str::from_utf8(bytes) {
        Ok(text) => serializer.serialize_str(text),
        Err(_) => {
            let mut map = serializer.serialize_map(Some(1))?;
            map.serialize_entry(BASE64_KEY, &Base64Text(bytes))?;
            map.end()
        }
    }
}

/// Deserialize a body written by [`serialize`]. A string is kept as its
/// bytes — taken whole when the deserializer hands over an owned string, one
/// copy of a borrowed one; an object is decoded from base64.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    deserializer.deserialize_any(BodyVisitor)
}

/// The length of `bytes` as [`serialize`] writes it with `serde_json`,
/// quotes and braces included: what one body adds to a request.
pub fn encoded_len(bytes: &[u8]) -> usize {
    match core::str::from_utf8(bytes) {
        Ok(text) => 2 + text.bytes().map(escaped_len).sum::<usize>(),
        // `{"base64":` + `"` + base64 + `"` + `}`
        Err(_) => BASE64_KEY.len() + 7 + bytes.len().div_ceil(3) * 4,
    }
}

/// A body borrowed for serialization, for bytes inside another container
/// (a list of `(path, bytes)`).
pub struct BodyRef<'a>(pub &'a [u8]);

impl Serialize for BodyRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize(self.0, serializer)
    }
}

/// A body decoded from the wire, for bytes inside another container.
pub struct BodyBuf(pub Vec<u8>);

impl<'de> Deserialize<'de> for BodyBuf {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize(deserializer).map(BodyBuf)
    }
}

/// How many characters `serde_json` writes for one byte of a string.
fn escaped_len(byte: u8) -> usize {
    match byte {
        b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
        0x00..=0x1f => 6,
        _ => 1,
    }
}

struct BodyVisitor;

impl<'de> Visitor<'de> for BodyVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(r#"a body: text as a string, or {"base64": "…"}"#)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Vec<u8>, E> {
        Ok(text.as_bytes().to_vec())
    }

    fn visit_string<E: de::Error>(self, text: String) -> Result<Vec<u8>, E> {
        Ok(text.into_bytes())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<u8>, A::Error> {
        let Some(BodyKey::Base64) = map.next_key::<BodyKey>()? else {
            return Err(de::Error::missing_field(BASE64_KEY));
        };
        let bytes = map.next_value::<Base64Bytes>()?.0;
        if map.next_key::<BodyKey>()?.is_some() {
            return Err(de::Error::duplicate_field(BASE64_KEY));
        }
        Ok(bytes)
    }
}

/// The binary body object's one field.
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum BodyKey {
    Base64,
}

/// Base64 decoded straight from the text the deserializer lends, with no
/// copy of the text (base64 never needs an escape, so `serde_json` lends it).
struct Base64Bytes(Vec<u8>);

impl<'de> Deserialize<'de> for Base64Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Base64Visitor;
        impl Visitor<'_> for Base64Visitor {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a base64 string")
            }
            fn visit_str<E: de::Error>(self, text: &str) -> Result<Vec<u8>, E> {
                base64::engine::general_purpose::STANDARD
                    .decode(text)
                    .map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Base64Visitor).map(Base64Bytes)
    }
}

/// Bytes as standard, padded base64, streamed with no `String` of it.
struct Base64Text<'a>(&'a [u8]);

impl Serialize for Base64Text<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&base64::display::Base64Display::new(
            self.0,
            &base64::engine::general_purpose::STANDARD,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Holder {
        #[serde(with = "crate::body_bytes")]
        body: Vec<u8>,
    }

    fn round_trip(body: &[u8]) -> String {
        let holder = Holder {
            body: body.to_vec(),
        };
        let json = serde_json::to_string(&holder).unwrap();
        let back: Holder = serde_json::from_str(&json).unwrap();
        assert_eq!(back, holder, "{json}");
        let back: Holder = serde_json::from_slice(json.as_bytes()).unwrap();
        assert_eq!(back, holder, "{json}");
        assert_eq!(
            encoded_len(body),
            json.len() - r#"{"body":}"#.len(),
            "encoded_len matches serde_json for {json}"
        );
        json
    }

    #[test]
    fn text_goes_as_itself_and_round_trips() {
        assert_eq!(
            round_trip(b"void main() {\n\tcolor = vec3(\"\\\");\n}\n"),
            r#"{"body":"void main() {\n\tcolor = vec3(\"\\\");\n}\n"}"#
        );
        assert_eq!(round_trip(b""), r#"{"body":""}"#);
        // Control characters take serde_json's six-character escape.
        assert_eq!(round_trip(b"\x01\x7f"), "{\"body\":\"\\u0001\x7f\"}");
        round_trip("üñî — ✓".as_bytes());
    }

    #[test]
    fn text_that_spells_base64_is_still_text() {
        // `abcd` is valid base64 of non-UTF-8 bytes: as a string, it is
        // always the four letters.
        assert_eq!(round_trip(b"abcd"), r#"{"body":"abcd"}"#);
        assert_eq!(round_trip(b"++++7/8="), r#"{"body":"++++7/8="}"#);
    }

    #[test]
    fn other_bytes_go_as_base64_in_an_object() {
        assert_eq!(
            round_trip(&[0xfb, 0xef, 0xbe, 0xef, 0xff]),
            r#"{"body":{"base64":"++++7/8="}}"#
        );
        let all: Vec<u8> = (0..=255u8).collect();
        round_trip(&all);
        round_trip(&[0xff]);
    }

    #[test]
    fn malformed_bodies_are_refused() {
        for bad in [
            r#"{"body":[1,2,3]}"#,
            r#"{"body":{}}"#,
            r#"{"body":{"base64":"!!"}}"#,
            r#"{"body":{"hex":"00"}}"#,
            r#"{"body":{"base64":"AA==","base64":"AA=="}}"#,
            r#"{"body":7}"#,
        ] {
            assert!(serde_json::from_str::<Holder>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn body_ref_and_buf_carry_bodies_inside_other_containers() {
        let list = vec![("a", BodyRef(b"text")), ("b", BodyRef(&[0xff]))];
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(json, r#"[["a","text"],["b",{"base64":"/w=="}]]"#);
        let back: Vec<(String, BodyBuf)> = serde_json::from_str(&json).unwrap();
        assert_eq!(back[0].1.0, b"text");
        assert_eq!(back[1].1.0, [0xff]);
    }
}
