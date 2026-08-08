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
use alloc::vec::Vec;

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
/// - `6` — shader GLSL entry points are dimension-explicit: the 2D entry is
///   `vec4 render_2d(vec2 pos)` (a bare `vec4 render(vec2 pos)` is now a
///   hard compile error — the D19/Q11 ruling). No 1D entry pre-dates this
///   format. Version-5 GLSL assets author the bare `render` signature and
///   are refused rather than migrated as JSON; the upgrader rewrites the
///   `.glsl` asset text itself (the entry function definition only).
/// - `5` — `bus:time` carries a **time product**, not raw seconds:
///   `ClockState` publishes a new `product` output on the channel (its
///   `seconds`/`delta_seconds` stay produced-but-unbound), `FluidDef.time`
///   and `PlaylistDef.time` are product-typed consumed slots rather than
///   f32 values, and `ShaderSlotDef` gained the `phasor`/`seconds` kinds
///   with a `phasor` config record. Version-4 artifacts author `"time": 0`
///   on fluid/playlist nodes and bind plain `f32` shader uniforms to
///   `bus:time`; both are refused rather than migrated.
/// - `4` — multi-endpoint output nodes: `OutputDef.endpoint` (one string)
///   became `channels` (a map of `{endpoint, count}` records), and endpoint
///   specs name the target device instead of a driver mechanism
///   (`ws281x:local:IO18`, likewise `button:local:*` / `radio:local:*`).
///   Version-3 outputs carry a top-level `endpoint` key and are refused.
///   Later in the same format: the container gained `target`, an advisory
///   `Option<String>` board-catalog id (`vendor/product`, the same strings
///   as `RegisteredDevice.board_id`) stored beside `author`/`license`.
///   Not a bump — same call as the `author`/`version`/`license`/`created`
///   provenance fields that joined the container earlier in this format
///   (P3 of the modules-impl roadmap): a purely additive optional
///   container field, never read by the engine, and the container's own
///   closed-vocabulary parser already refuses it loudly on an
///   unaware reader (no version field is needed for that refusal to be
///   loud rather than silent — see the module-level doc above). Reserve
///   the bump for changes that alter the *meaning* of already-authored
///   data, as this format's own `channels` change did.
///   Later still in the same format: the container gained `kind`/`exports`
///   (module authoring unit, P1): an optional authored project kind
///   (`"general"` (default, absent), `"pattern"`, `"show"`, `"rig"`) and,
///   for the two library kinds, an `exports` list of module folder names.
///   Not a bump for the same reason `target` was not — purely additive,
///   never read by the engine, and the container's closed-vocabulary
///   parser already turns an unaware reader's encounter with either key
///   into a loud parse error rather than a silent drop.
/// - `3` — project/module mitosis: `project.json` became the non-node
///   container manifest (`format`/`uid`/`name`), and the root module node
///   moved to `module.json` (kind `Module`, renamed from `project` in the
///   same train). Version-2 roots are single-file kind-tagged artifacts and
///   are refused.
/// - `2` — shader nodes replaced the `glsl_opts` record (`add_sub`/`mul`/
///   `div` Q32 mode slots) with a single `float_mode` slot. Artifacts at
///   version `1` are refused, not migrated.
pub const PROJECT_FORMAT_VERSION: u32 = 7;

/// A project's authored kind (module authoring unit, P1 —
/// `docs/design/modules.md`): the default general project, or one of two
/// *library* kinds that export named modules for other projects to import.
///
/// Resolved from the manifest's flat `kind`/`exports` JSON keys via
/// [`ProjectManifest::project_kind`] — the manifest itself keeps the raw
/// strings ([`ProjectManifest::kind_raw`]/[`ProjectManifest::exports_raw`])
/// so an unresolved manifest can still round-trip byte-identically. The
/// engine never reads this (D14 of the module authoring plan); it drives
/// Studio-side lint (P2) and UI (P3+) only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProjectKind {
    /// An ordinary authored project. `kind` absent on disk.
    #[default]
    General,
    /// A library project: authors named modules other projects import,
    /// listed by folder name in `exports`.
    Pattern { exports: Vec<String> },
    /// A show project: sequences/plays other projects. Exports nothing.
    Show,
    /// A library project like [`Self::Pattern`], for physical rig modules.
    Rig { exports: Vec<String> },
}

/// Parsed `project.json` container manifest.
///
/// All fields are optional at the parse layer; the loader format gate is
/// what enforces `format` presence/match ([`PROJECT_FORMAT_VERSION`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectManifest {
    /// Authored format version; see [`PROJECT_FORMAT_VERSION`].
    pub format: Option<u32>,
    /// Stable project identity (`prj…`, base-32), minted by the library
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
    /// Authored project kind, as the raw JSON string (`"general"` |
    /// `"pattern"` | `"show"` | `"rig"`), or `None` when the key is absent
    /// (the common case: an ordinary project, equivalent to `"general"`).
    /// [`Self::read_json`] validates this against the closed vocabulary,
    /// but keeps it raw rather than resolved — read [`ProjectKind`] through
    /// [`Self::project_kind`], and patch it through [`Self::set_kind`].
    pub kind_raw: Option<String>,
    /// Authored module export list: plain folder names under the project,
    /// each one another project imports by
    /// (`docs/design/modules.md`). Meaningful only when [`Self::kind_raw`]
    /// is `"pattern"` or `"rig"` — [`Self::read_json`] refuses `exports`
    /// alongside any other kind, including absent.
    pub exports_raw: Option<Vec<String>>,
    /// Advisory board target: a board catalog id in the registry's
    /// `vendor/product` vocabulary (e.g. `espressif/esp32-c6-devkitc-1`),
    /// the same strings as `RegisteredDevice.board_id`. Feeds generation,
    /// the load-time mismatch warning, and sim board inheritance — never
    /// read by the engine. Provenance-tier metadata beside `author` /
    /// `license`; not validated against the board catalog here (`lpc-model`
    /// carries no catalog dependency).
    pub target: Option<String>,
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

    /// Resolve the authored `kind`/`exports` keys into [`ProjectKind`]
    /// ([`ProjectKind::General`] when `kind` is absent).
    ///
    /// Infallible: [`Self::read_json`] already refused any manifest whose
    /// `kind`/`exports` combination this cannot represent (an unknown
    /// `kind` string, or `exports` without a library kind), so by the time
    /// a manifest exists via the parser its `kind_raw`/`exports_raw` are
    /// always one of the four known shapes. A manifest built directly
    /// (bypassing the parser) with an out-of-vocabulary `kind_raw` resolves
    /// to [`ProjectKind::General`] rather than panicking.
    pub fn project_kind(&self) -> ProjectKind {
        match self.kind_raw.as_deref() {
            Some("pattern") => ProjectKind::Pattern {
                exports: self.exports_raw.clone().unwrap_or_default(),
            },
            Some("show") => ProjectKind::Show,
            Some("rig") => ProjectKind::Rig {
                exports: self.exports_raw.clone().unwrap_or_default(),
            },
            None | Some("general") | Some(_) => ProjectKind::General,
        }
    }

    /// Set the authored project kind, keeping [`Self::kind_raw`]/
    /// [`Self::exports_raw`] consistent with it: [`ProjectKind::General`]
    /// clears both keys (the default needs no authored trace), the two
    /// library kinds author their `exports` list, and `Show` clears
    /// `exports` (it is never a library kind).
    pub fn set_kind(&mut self, kind: ProjectKind) {
        match kind {
            ProjectKind::General => {
                self.kind_raw = None;
                self.exports_raw = None;
            }
            ProjectKind::Pattern { exports } => {
                self.kind_raw = Some(String::from("pattern"));
                self.exports_raw = Some(exports);
            }
            ProjectKind::Show => {
                self.kind_raw = Some(String::from("show"));
                self.exports_raw = None;
            }
            ProjectKind::Rig { exports } => {
                self.kind_raw = Some(String::from("rig"));
                self.exports_raw = Some(exports);
            }
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
                    "kind" => manifest.kind_raw = Some(read_string(&mut source, "kind")?),
                    "exports" => {
                        manifest.exports_raw = Some(read_string_array(&mut source, "exports")?);
                    }
                    "target" => manifest.target = Some(read_string(&mut source, "target")?),
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
        validate_kind_and_exports(&manifest)?;
        Ok(manifest)
    }

    /// Write the manifest as canonical authored JSON: pretty-printed, fixed
    /// field order (`format`, `uid`, `name`, `author`, `version`,
    /// `license`, `created`, `kind`, `exports`, `target`), absent fields
    /// omitted, trailing newline. Deterministic so unchanged models produce
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
        if let Some(kind) = &self.kind_raw {
            field("kind", kind, true, &mut out);
        }
        if let Some(exports) = &self.exports_raw {
            let mut array = String::from("[");
            for (index, item) in exports.iter().enumerate() {
                if index > 0 {
                    array.push(',');
                }
                array.push_str("\n    ");
                push_json_string(&mut array, item);
            }
            if !exports.is_empty() {
                array.push_str("\n  ");
            }
            array.push(']');
            field("exports", &array, false, &mut out);
        }
        if let Some(target) = &self.target {
            field("target", target, true, &mut out);
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

/// Read a JSON array of plain strings (the `exports` shape): each item a
/// module folder name, validated as a plain path segment as it is read.
fn read_string_array(
    source: &mut JsonSyntaxSource<'_>,
    field: &str,
) -> Result<Vec<String>, ManifestParseError> {
    let syntax_error =
        |error: crate::slot_codec::SyntaxError| ManifestParseError::Syntax(error.to_string());
    match source.next_event().map_err(syntax_error)? {
        Some(SyntaxEvent::StartArray { .. }) => {}
        _ => {
            return Err(ManifestParseError::Syntax(alloc::format!(
                "field `{field}` must be an array of strings"
            )));
        }
    }
    let mut items = Vec::new();
    loop {
        match source.next_event().map_err(syntax_error)? {
            Some(SyntaxEvent::EndArray { .. }) => break,
            Some(SyntaxEvent::StringChunk { text, is_last, .. }) => {
                let mut value = text;
                let mut done = is_last;
                while !done {
                    match source.next_event().map_err(syntax_error)? {
                        Some(SyntaxEvent::StringChunk {
                            text: next,
                            is_last: next_done,
                            ..
                        }) => {
                            value.push_str(&next);
                            done = next_done;
                        }
                        _ => {
                            return Err(ManifestParseError::Syntax(alloc::format!(
                                "field `{field}` must be an array of strings"
                            )));
                        }
                    }
                }
                validate_export_segment(field, &value)?;
                items.push(value);
            }
            _ => {
                return Err(ManifestParseError::Syntax(alloc::format!(
                    "field `{field}` must be an array of strings"
                )));
            }
        }
    }
    Ok(items)
}

/// An export entry names a module by its folder — a plain path segment,
/// never a path: nonempty, no `/`, no `..`. Whether the folder actually
/// exists is not a parse concern (that's lint, P2).
fn validate_export_segment(field: &str, segment: &str) -> Result<(), ManifestParseError> {
    if segment.is_empty() {
        return Err(ManifestParseError::Syntax(alloc::format!(
            "field `{field}` entries must be nonempty"
        )));
    }
    if segment.contains('/') {
        return Err(ManifestParseError::Syntax(alloc::format!(
            "field `{field}` entry {segment:?} must be a plain folder name, not a path"
        )));
    }
    if segment.contains("..") {
        return Err(ManifestParseError::Syntax(alloc::format!(
            "field `{field}` entry {segment:?} must not contain `..`"
        )));
    }
    Ok(())
}

/// Cross-field validation for the closed `kind`/`exports` vocabulary,
/// applied once the whole object has been read (JSON key order is not
/// authored order, so `exports` may have arrived before `kind`).
fn validate_kind_and_exports(manifest: &ProjectManifest) -> Result<(), ManifestParseError> {
    if let Some(kind) = &manifest.kind_raw {
        if !matches!(kind.as_str(), "general" | "pattern" | "show" | "rig") {
            return Err(ManifestParseError::Syntax(alloc::format!(
                "unknown project kind {kind:?}; must be one of \"general\", \"pattern\", \"show\", \"rig\""
            )));
        }
    }
    if manifest.exports_raw.is_some()
        && !matches!(manifest.kind_raw.as_deref(), Some("pattern") | Some("rig"))
    {
        return Err(ManifestParseError::Syntax(String::from(
            "field `exports` requires `kind` to be \"pattern\" or \"rig\"",
        )));
    }
    Ok(())
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
            uid: Some(String::from("prj0000000000000042")),
            name: Some(String::from("Porch sign")),
            author: Some(String::from("Yona")),
            version: Some(String::from("0.1")),
            license: Some(String::from("CC0-1.0")),
            created: Some(String::from("2026-08-01")),
            kind_raw: None,
            exports_raw: None,
            target: Some(String::from("espressif/esp32-c6-devkitc-1")),
        };
        let text = manifest.write_json();
        assert_eq!(
            text,
            "{\n  \"format\": 7,\n  \"uid\": \"prj0000000000000042\",\n  \"name\": \"Porch sign\",\n  \"author\": \"Yona\",\n  \"version\": \"0.1\",\n  \"license\": \"CC0-1.0\",\n  \"created\": \"2026-08-01\",\n  \"target\": \"espressif/esp32-c6-devkitc-1\"\n}\n"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(read.write_json(), text);
    }

    /// P02: `target` present round-trips, and its absence (the common case
    /// — an untargeted project) serializes to nothing, exactly like the
    /// other optional provenance fields.
    #[test]
    fn manifest_target_present_round_trips() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            target: Some(String::from("seeed/xiao-esp32-c6")),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        assert_eq!(
            text,
            "{\n  \"format\": 7,\n  \"target\": \"seeed/xiao-esp32-c6\"\n}\n"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(read.target.as_deref(), Some("seeed/xiao-esp32-c6"));
    }

    #[test]
    fn manifest_target_absent_round_trips() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        assert!(
            !text.contains("target"),
            "an untargeted project must not author a target key: {text}"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read.target, None);
    }

    #[test]
    fn manifest_absent_fields_serialize_to_nothing() {
        let manifest = ProjectManifest {
            format: Some(4),
            ..ProjectManifest::default()
        };
        assert_eq!(manifest.write_json(), "{\n  \"format\": 4\n}\n");
        assert_eq!(ProjectManifest::default().write_json(), "{}\n");
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let err = ProjectManifest::read_json(r#"{ "format": 4, "nodes": {} }"#)
            .expect_err("nodes is not a container field");
        assert_eq!(
            err,
            ManifestParseError::UnknownField {
                field: String::from("nodes")
            }
        );
        // P1: `kind` is now a KNOWN container key with a closed value set,
        // not an unknown field — but the pre-mitosis root's value
        // (`"Module"`, a node-kind tag, never a project kind) is still
        // outside that vocabulary, so it must still fail loudly, just with
        // a different diagnosis than an unknown *key*: a clear message
        // naming the four allowed project kinds.
        let err = ProjectManifest::read_json(r#"{ "kind": "Module", "format": 2 }"#)
            .expect_err("Module is not a known project kind");
        assert!(
            err.to_string().contains("general")
                && err.to_string().contains("pattern")
                && err.to_string().contains("show")
                && err.to_string().contains("rig")
                && err.to_string().contains("Module"),
            "{err}"
        );
    }

    /// P1: `kind`/`exports` round-trip byte-identically for both library
    /// kinds, and the fixed writer order places them between `created` and
    /// `target`.
    #[test]
    fn manifest_kind_and_exports_round_trip() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            created: Some(String::from("2026-08-07")),
            kind_raw: Some(String::from("pattern")),
            exports_raw: Some(alloc::vec![String::from("chase"), String::from("sparkle")]),
            target: Some(String::from("espressif/esp32-c6-devkitc-1")),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        assert_eq!(
            text,
            "{\n  \"format\": 6,\n  \"created\": \"2026-08-07\",\n  \"kind\": \"pattern\",\n  \"exports\": [\n    \"chase\",\n    \"sparkle\"\n  ],\n  \"target\": \"espressif/esp32-c6-devkitc-1\"\n}\n"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(read.write_json(), text);
        assert_eq!(
            read.project_kind(),
            ProjectKind::Pattern {
                exports: alloc::vec![String::from("chase"), String::from("sparkle")]
            }
        );
    }

    /// P1: `rig` is the other library kind, and its `exports` list can be
    /// empty (authored but with nothing exported yet).
    #[test]
    fn manifest_rig_kind_with_empty_exports_round_trips() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            kind_raw: Some(String::from("rig")),
            exports_raw: Some(Vec::new()),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        assert_eq!(
            text,
            "{\n  \"format\": 6,\n  \"kind\": \"rig\",\n  \"exports\": []\n}\n"
        );
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(
            read.project_kind(),
            ProjectKind::Rig {
                exports: Vec::new()
            }
        );
    }

    /// P1: `show` is a real kind but never a library kind — it carries no
    /// `exports` key at all.
    #[test]
    fn manifest_show_kind_round_trips_without_exports() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            kind_raw: Some(String::from("show")),
            ..ProjectManifest::default()
        };
        let text = manifest.write_json();
        assert_eq!(text, "{\n  \"format\": 6,\n  \"kind\": \"show\"\n}\n");
        let read = ProjectManifest::read_json(&text).expect("read back");
        assert_eq!(read, manifest);
        assert_eq!(read.project_kind(), ProjectKind::Show);
    }

    /// P1: `kind` absent resolves to `General` — the common, untargeted
    /// case, exactly like the other optional fields.
    #[test]
    fn manifest_kind_absent_resolves_to_general() {
        let manifest = ProjectManifest {
            format: Some(PROJECT_FORMAT_VERSION),
            ..ProjectManifest::default()
        };
        assert_eq!(manifest.project_kind(), ProjectKind::General);
        assert!(!manifest.write_json().contains("kind"));
    }

    /// P1: `exports` is only meaningful alongside a library kind —
    /// authoring it with `kind` absent, or with a non-library kind, is a
    /// parse error naming the requirement, not a silently-ignored field.
    #[test]
    fn manifest_rejects_exports_without_library_kind() {
        let err = ProjectManifest::read_json(r#"{ "format": 5, "exports": ["chase"] }"#)
            .expect_err("exports without any kind at all");
        assert!(
            err.to_string().contains("pattern") && err.to_string().contains("rig"),
            "{err}"
        );

        let err =
            ProjectManifest::read_json(r#"{ "format": 5, "kind": "show", "exports": ["chase"] }"#)
                .expect_err("exports alongside a non-library kind");
        assert!(
            err.to_string().contains("pattern") && err.to_string().contains("rig"),
            "{err}"
        );
    }

    /// P1: an unrecognized `kind` string is refused with a message naming
    /// the closed set of allowed values.
    #[test]
    fn manifest_rejects_unknown_kind_value() {
        let err = ProjectManifest::read_json(r#"{ "format": 5, "kind": "diorama" }"#)
            .expect_err("diorama is not a project kind");
        assert!(err.to_string().contains("diorama"), "{err}");
        assert!(
            err.to_string().contains("general")
                && err.to_string().contains("pattern")
                && err.to_string().contains("show")
                && err.to_string().contains("rig"),
            "{err}"
        );
    }

    /// P1: an export entry must be a plain folder name — not a path, not
    /// empty, and not a `..` escape.
    #[test]
    fn manifest_rejects_malformed_export_segments() {
        for bad in [r#""""#, r#""a/b""#, r#""..""#, r#""../escape""#] {
            let text =
                alloc::format!(r#"{{ "format": 5, "kind": "pattern", "exports": [{bad}] }}"#);
            ProjectManifest::read_json(&text).expect_err(&alloc::format!("bad segment {bad}"));
        }
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
