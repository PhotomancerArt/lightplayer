//! An order- and text-preserving JSON document tree.
//!
//! The migrator edits authored files that a human will read again in a diff,
//! so the two properties `serde_json::Value` cannot give us both matter:
//!
//! - **Key order.** `serde_json::Map` is a `BTreeMap` unless the crate-wide
//!   `preserve_order` feature is on. Enabling that feature here would enable
//!   it for *every* host crate built in the same cargo invocation (feature
//!   unification is per-build, not per-crate) — including `schemars`, whose
//!   generated schemas are committed in sorted key order and gated by
//!   `just schema-check`. A migrator must not be able to churn the schema
//!   corpus, so the feature stays off and order lives in this tree instead.
//! - **Numeric spelling.** `Value` stores numbers as `f64`, and re-emitting
//!   one goes through ryū: `0.00003` comes back as `3e-5`. That is a real
//!   diff in `examples/fluid/fluid.json`, a file the v4→v5 step *does*
//!   rewrite. Scalars are therefore kept as their original source text.
//!
//! Scalars (numbers, strings, booleans, `null`) are stored verbatim as the
//! bytes `serde_json` captured for them; only containers are structural.

use serde_json::value::RawValue;

/// One JSON node: a container, or a scalar's verbatim source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonNode {
    /// Object members in authored order. Duplicate keys are preserved as
    /// authored (`serde_json` accepts them; we do not silently collapse).
    Object(Vec<(String, JsonNode)>),
    Array(Vec<JsonNode>),
    /// The exact source text of a number, string, boolean, or `null`.
    Scalar(String),
}

impl JsonNode {
    /// Parse a whole JSON document. Trailing content is an error, so this
    /// doubles as the "is this even JSON" gate.
    pub fn parse(text: &str) -> Result<Self, JsonError> {
        let raw: Box<RawValue> = serde_json::from_str(text).map_err(JsonError::from_serde)?;
        Self::from_raw(&raw)
    }

    /// Render as 2-space-indented pretty JSON with a trailing newline —
    /// the shape every authored artifact in the corpus is written in.
    pub fn to_pretty_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        self.write_pretty(&mut out, 0);
        out.push('\n');
        out.into_bytes()
    }

    pub fn object(&self) -> Option<&Vec<(String, JsonNode)>> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }

    pub fn object_mut(&mut self) -> Option<&mut Vec<(String, JsonNode)>> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsonNode> {
        self.object()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, node)| node)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut JsonNode> {
        self.object_mut()?
            .iter_mut()
            .find(|(name, _)| name == key)
            .map(|(_, node)| node)
    }

    /// Replace `key`'s value in place (keeping its position), or append it.
    pub fn set(&mut self, key: &str, value: JsonNode) {
        let Some(members) = self.object_mut() else {
            return;
        };
        match members.iter_mut().find(|(name, _)| name == key) {
            Some(slot) => slot.1 = value,
            None => members.push((key.to_owned(), value)),
        }
    }

    /// Rename `key` to `new_key` in place, keeping its position and value.
    /// No-op when `key` is absent.
    pub fn rename_key(&mut self, key: &str, new_key: &str) {
        let Some(members) = self.object_mut() else {
            return;
        };
        if let Some(slot) = members.iter_mut().find(|(name, _)| name == key) {
            slot.0 = new_key.to_owned();
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<JsonNode> {
        let members = self.object_mut()?;
        let index = members.iter().position(|(name, _)| name == key)?;
        Some(members.remove(index).1)
    }

    /// This scalar decoded as a string, if it is a JSON string.
    pub fn as_str(&self) -> Option<String> {
        let Self::Scalar(text) = self else {
            return None;
        };
        serde_json::from_str::<String>(text).ok()
    }

    /// This scalar decoded as an unsigned integer, if it is one.
    pub fn as_u32(&self) -> Option<u32> {
        let Self::Scalar(text) = self else {
            return None;
        };
        text.parse::<u32>().ok()
    }

    /// Whether this node is a JSON number (of any spelling).
    pub fn is_number(&self) -> bool {
        let Self::Scalar(text) = self else {
            return false;
        };
        matches!(text.as_bytes().first().copied(), Some(b'-' | b'0'..=b'9'))
    }

    /// A scalar holding a JSON string with `value`'s contents.
    pub fn string(value: &str) -> Self {
        Self::Scalar(serde_json::Value::String(value.to_string()).to_string())
    }

    /// A scalar holding `value` as a JSON integer.
    pub fn u32(value: u32) -> Self {
        Self::Scalar(value.to_string())
    }

    /// Whether `key` holds a JSON string equal to `value`.
    pub fn has_string(&self, key: &str, value: &str) -> bool {
        self.get(key).and_then(JsonNode::as_str).as_deref() == Some(value)
    }

    fn from_raw(raw: &RawValue) -> Result<Self, JsonError> {
        let text = raw.get();
        match text.trim_start().as_bytes().first().copied() {
            Some(b'{') => {
                let members: OrderedObject =
                    serde_json::from_str(text).map_err(JsonError::from_serde)?;
                let mut out = Vec::with_capacity(members.0.len());
                for (key, value) in &members.0 {
                    out.push((key.clone(), Self::from_raw(value)?));
                }
                Ok(Self::Object(out))
            }
            Some(b'[') => {
                let items: Vec<Box<RawValue>> =
                    serde_json::from_str(text).map_err(JsonError::from_serde)?;
                let mut out = Vec::with_capacity(items.len());
                for item in &items {
                    out.push(Self::from_raw(item)?);
                }
                Ok(Self::Array(out))
            }
            _ => Ok(Self::Scalar(text.to_owned())),
        }
    }

    fn write_pretty(&self, out: &mut String, depth: usize) {
        match self {
            Self::Scalar(text) => out.push_str(text),
            Self::Array(items) if items.is_empty() => out.push_str("[]"),
            Self::Array(items) => {
                out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    push_indent(out, depth + 1);
                    item.write_pretty(out, depth + 1);
                    if index + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                push_indent(out, depth);
                out.push(']');
            }
            Self::Object(members) if members.is_empty() => out.push_str("{}"),
            Self::Object(members) => {
                out.push_str("{\n");
                for (index, (key, value)) in members.iter().enumerate() {
                    push_indent(out, depth + 1);
                    out.push_str(&serde_json::Value::String(key.clone()).to_string());
                    out.push_str(": ");
                    value.write_pretty(out, depth + 1);
                    if index + 1 < members.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                push_indent(out, depth);
                out.push('}');
            }
        }
    }
}

/// A JSON document this crate could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub detail: String,
}

impl JsonError {
    fn from_serde(error: serde_json::Error) -> Self {
        Self {
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for JsonError {}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Object members in source order. `serde_json` hands map entries to the
/// visitor in document order; `serde_json::Map` is what loses that, not the
/// parser, so collecting into a `Vec` keeps it.
struct OrderedObject(Vec<(String, Box<RawValue>)>);

impl<'de> serde::Deserialize<'de> for OrderedObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(OrderedObjectVisitor)
    }
}

struct OrderedObjectVisitor;

impl<'de> serde::de::Visitor<'de> for OrderedObjectVisitor {
    type Value = OrderedObject;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut members = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
            members.push((key, value));
        }
        Ok(OrderedObject(members))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_order_survives_a_round_trip() {
        let text = "{\n  \"zebra\": 1,\n  \"apple\": 2\n}\n";
        let node = JsonNode::parse(text).unwrap();
        assert_eq!(node.to_pretty_bytes(), text.as_bytes());
    }

    #[test]
    fn numeric_spelling_survives_a_round_trip() {
        // `serde_json::Value` would re-emit these as 3e-5, 100.0 and 0.0 —
        // the churn this DOM exists to avoid.
        let text =
            "{\n  \"tiny\": 0.00003,\n  \"round\": 100.0,\n  \"zero\": 0,\n  \"neg\": -1.5e10\n}\n";
        let node = JsonNode::parse(text).unwrap();
        assert_eq!(node.to_pretty_bytes(), text.as_bytes());
    }

    #[test]
    fn nested_containers_reindent_to_two_spaces() {
        let node = JsonNode::parse("{\"a\":[1,{\"b\":true}],\"c\":{},\"d\":[]}").unwrap();
        assert_eq!(
            String::from_utf8(node.to_pretty_bytes()).unwrap(),
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": true\n    }\n  ],\n  \"c\": {},\n  \"d\": []\n}\n"
        );
    }

    #[test]
    fn strings_keep_their_escapes() {
        let text = "{\n  \"s\": \"a\\\"b\\nc\"\n}\n";
        let node = JsonNode::parse(text).unwrap();
        assert_eq!(node.get("s").unwrap().as_str().unwrap(), "a\"b\nc");
        assert_eq!(node.to_pretty_bytes(), text.as_bytes());
    }

    #[test]
    fn trailing_content_is_an_error() {
        assert!(JsonNode::parse("{} trailing").is_err());
        assert!(JsonNode::parse("not json at all").is_err());
    }

    #[test]
    fn edits_keep_their_position() {
        let mut node = JsonNode::parse("{\"a\":1,\"b\":2,\"c\":3}").unwrap();
        node.set("b", JsonNode::u32(9));
        node.rename_key("c", "z");
        assert_eq!(
            node.remove("a").unwrap(),
            JsonNode::Scalar(String::from("1"))
        );
        assert_eq!(
            String::from_utf8(node.to_pretty_bytes()).unwrap(),
            "{\n  \"b\": 9,\n  \"z\": 3\n}\n"
        );
    }
}
