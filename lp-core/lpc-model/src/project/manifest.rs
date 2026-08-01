//! `project.json` container manifest: the project's workspace identity.
//!
//! The container manifest is **not a node artifact** (docs/design/modules.md
//! §1/§6): it carries the workspace concerns — `format`, `uid`, `name` —
//! while the root module node lives in `module.json`. It is deliberately NOT
//! a `#[derive(Slotted)]` type: the container is read by a streaming
//! [`crate::slot_codec::JsonSyntaxSource`] probe and written by a hand-rolled
//! deterministic writer, so no second shape/codec surface links into device
//! firmware for three fields (serde surface is the flash lever).
//!
//! Reading is strict about shape (root must be an object; known fields must
//! have the right type) and strict about vocabulary: unknown top-level keys
//! are an error, which is what makes read→modify→write patching lossless by
//! construction (there is nothing the writer could drop).

use alloc::string::{String, ToString};

use crate::slot_codec::{JsonSyntaxSource, SyntaxEvent, SyntaxEventSource};

/// Monotonic format version of an authored project (the container and every
/// artifact transitively inside it).
///
/// The container manifest (`project.json`) carries this as its `format` key;
/// module and node files are versioned transitively through their project's
/// container. Loaders reject projects whose format is missing or does not
/// match, so bump this when making a format-breaking change to authored
/// artifacts (alpha posture: bump and refuse, never migrate).
///
/// History:
/// - `3` — project/module mitosis: `project.json` became the non-node
///   container manifest (`format`/`uid`/`name`), and the root module node
///   moved to `module.json` (kind `Module`, renamed from `project` in the
///   same train). Version-2 roots are single-file kind-tagged artifacts and
///   are refused.
/// - `2` — shader nodes replaced the `glsl_opts` record (`add_sub`/`mul`/
///   `div` Q32 mode slots) with a single `float_mode` slot. Artifacts at
///   version `1` are refused, not migrated.
pub const PROJECT_FORMAT_VERSION: u32 = 3;

/// Parsed `project.json` container manifest.
///
/// All fields are optional at the parse layer; the loader format gate is
/// what enforces `format` presence/match ([`PROJECT_FORMAT_VERSION`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectManifest {
    /// Authored format version; see [`PROJECT_FORMAT_VERSION`].
    pub format: Option<u32>,
    /// Stable project identity (`prj_…`, base-62), minted by the library
    /// when a project enters it. Parity checks, history, and device
    /// associations key off it.
    pub uid: Option<String>,
    /// Human-readable project name — the Studio project pane's title.
    pub name: Option<String>,
    /// Provenance (§8, settled Q7): author attribution.
    pub author: Option<String>,
    /// Provenance: authored version string; no semver semantics yet.
    pub version: Option<String>,
    /// Provenance: license identifier (e.g. `"CC0-1.0"`).
    pub license: Option<String>,
    /// Provenance: ISO date the project was created.
    pub created: Option<String>,
}

impl ProjectManifest {
    /// Manifest for a freshly authored project at the current format.
    pub fn new_current(name: &str) -> Self {
        Self {
            format: Some(PROJECT_FORMAT_VERSION),
            name: Some(String::from(name)),
            ..Self::default()
        }
    }

    /// Parse a `project.json` container manifest.
    ///
    /// Streaming, no value tree; strict: the root must be a JSON object,
    /// known fields must carry the right type, and unknown top-level keys
    /// are rejected (which keeps read→modify→write lossless).
    pub fn read_json(text: &str) -> Result<Self, ManifestParseError> {
        let syntax_error =
            |error: crate::slot_codec::SyntaxError| ManifestParseError::Syntax(error.to_string());

        let mut source = JsonSyntaxSource::new(text).map_err(syntax_error)?;
        match source.next_event().map_err(syntax_error)? {
            Some(SyntaxEvent::StartObject { .. }) => {}
            _ => {
                return Err(ManifestParseError::Syntax(String::from(
                    "project manifest root must be an object",
                )));
            }
        }

        let mut manifest = Self::default();
        loop {
            match source.next_event().map_err(syntax_error)? {
                Some(SyntaxEvent::Prop { name, .. }) => match name.as_str() {
                    "format" => {
                        let value = match source.next_event().map_err(syntax_error)? {
                            Some(SyntaxEvent::Number { text, .. }) => text.parse::<u32>().ok(),
                            _ => None,
                        };
                        let Some(value) = value else {
                            return Err(ManifestParseError::Syntax(String::from(
                                "field `format` must be an unsigned integer",
                            )));
                        };
                        manifest.format = Some(value);
                    }
                    "uid" => manifest.uid = Some(read_string(&mut source, "uid")?),
                    "name" => manifest.name = Some(read_string(&mut source, "name")?),
                    "author" => manifest.author = Some(read_string(&mut source, "author")?),
                    "version" => manifest.version = Some(read_string(&mut source, "version")?),
                    "license" => manifest.license = Some(read_string(&mut source, "license")?),
                    "created" => manifest.created = Some(read_string(&mut source, "created")?),
                    other => {
                        return Err(ManifestParseError::UnknownField {
                            field: other.to_string(),
                        });
                    }
                },
                Some(SyntaxEvent::EndObject { .. }) => break,
                Some(_) => {
                    return Err(ManifestParseError::Syntax(String::from(
                        "unexpected token in project manifest",
                    )));
                }
                None => {
                    return Err(ManifestParseError::Syntax(String::from(
                        "unterminated project manifest object",
                    )));
                }
            }
        }
        Ok(manifest)
    }

    /// Write the manifest as canonical authored JSON: pretty-printed, fixed
    /// field order (`format`, `uid`, `name`, `author`, `version`,
    /// `license`, `created`), absent fields omitted,
    /// trailing newline. Deterministic so unchanged models produce
    /// byte-identical files.
    pub fn write_json(&self) -> String {
        let mut out = String::from("{");
        let mut first = true;
        let mut field = |name: &str, value: &str, quote: bool, out: &mut String| {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str("\n  \"");
            out.push_str(name);
            out.push_str("\": ");
            if quote {
                push_json_string(out, value);
            } else {
                out.push_str(value);
            }
        };
        if let Some(format) = self.format {
            field("format", &format.to_string(), false, &mut out);
        }
        if let Some(uid) = &self.uid {
            field("uid", uid, true, &mut out);
        }
        if let Some(name) = &self.name {
            field("name", name, true, &mut out);
        }
        if let Some(author) = &self.author {
            field("author", author, true, &mut out);
        }
        if let Some(version) = &self.version {
            field("version", version, true, &mut out);
        }
        if let Some(license) = &self.license {
            field("license", license, true, &mut out);
        }
        if let Some(created) = &self.created {
            field("created", created, true, &mut out);
        }
        if first {
            out.push_str("}\n");
        } else {
            out.push_str("\n}\n");
        }
        out
    }
}

/// Failure parsing a `project.json` container manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestParseError {
    /// Malformed JSON or a wrong-typed known field.
    Syntax(String),
    /// A top-level key outside the container vocabulary.
    UnknownField { field: String },
}

impl core::fmt::Display for ManifestParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Syntax(error) => f.write_str(error),
            Self::UnknownField { field } => {
                write!(f, "unknown project manifest field `{field}`")
            }
        }
    }
}

impl core::error::Error for ManifestParseError {}

fn read_string(
    source: &mut JsonSyntaxSource<'_>,
    field: &str,
) -> Result<String, ManifestParseError> {
    let syntax_error =
        |error: crate::slot_codec::SyntaxError| ManifestParseError::Syntax(error.to_string());
    let mut value = String::new();
    loop {
        match source.next_event().map_err(syntax_error)? {
            Some(SyntaxEvent::StringChunk { text, is_last, .. }) => {
                value.push_str(&text);
                if is_last {
                    return Ok(value);
                }
            }
            _ => {
                return Err(ManifestParseError::Syntax(alloc::format!(
                    "field `{field}` must be a string"
                )));
            }
        }
    }
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if ch <= '\u{1f}' => {
                let n = ch as u8;
                out.push_str("\\u00");
                let hex = |nibble: u8| char::from_digit(u32::from(nibble), 16).unwrap_or('0');
                out.push(hex(n >> 4));
                out.push(hex(n & 0x0f));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_byte_identically() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            uid: Some(String::from("prj_0000000000000042")),
            name: Some(String::from("Porch sign")),
            author: Some(String::from("Yona")),
            version: Some(String::from("0.1")),
            license: Some(String::from("CC0-1.0")),
            created: Some(String::from("2026-08-01")),
        };
        let text = manifest.write_json();
        assert_eq!(
            text,
            "{\n  \"format\": 3,\n  \"uid\": \"prj_0000000000000042\",\n  \"name\": \"Porch sign\",\n  \"author\": \"Yona\",\n  \"version\": \"0.1\",\n  \"license\": \"CC0-1.0\",\n  \"created\": \"2026-08-01\"\n}\n"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(read.write_json(), text);
    }

    #[test]
    fn manifest_absent_fields_serialize_to_nothing() {
        let manifest = ProjectManifest {
            format: Some(3),
            ..ProjectManifest::default()
        };
        assert_eq!(manifest.write_json(), "{\n  \"format\": 3\n}\n");
        assert_eq!(ProjectManifest::default().write_json(), "{}\n");
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let err = ProjectManifest::read_json(r#"{ "format": 3, "nodes": {} }"#)
            .expect_err("nodes is not a container field");
        assert_eq!(
            err,
            ManifestParseError::UnknownField {
                field: String::from("nodes")
            }
        );
        // The pre-mitosis root is diagnosable by its `kind` key.
        let err = ProjectManifest::read_json(r#"{ "kind": "Module", "format": 2 }"#)
            .expect_err("kind is not a container field");
        assert_eq!(
            err,
            ManifestParseError::UnknownField {
                field: String::from("kind")
            }
        );
    }

    #[test]
    fn manifest_rejects_wrong_typed_fields() {
        let err = ProjectManifest::read_json(r#"{ "format": "three" }"#).expect_err("string");
        assert!(err.to_string().contains("unsigned integer"), "{err}");
        let err = ProjectManifest::read_json(r#"{ "name": 7 }"#).expect_err("number name");
        assert!(err.to_string().contains("string"), "{err}");
        let err = ProjectManifest::read_json(r#"[1]"#).expect_err("array root");
        assert!(err.to_string().contains("object"), "{err}");
    }

    #[test]
    fn manifest_name_escapes_round_trip() {
        let manifest = ProjectManifest {
            format: Some(3),
            name: Some(String::from("a \"b\"\\\n\tc")),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        let read = ProjectManifest::read_json(&text).expect("read escaped");
        assert_eq!(read, manifest);
    }
}
