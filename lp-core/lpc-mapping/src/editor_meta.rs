//! The project-level **editor metadata** document (`editor.json`): where
//! each fixture sits on the editor's conceptual Arrange canvas.
//!
//! One document per project, sibling of `project.json`, owned entirely by
//! the editor. It stores per-node, per-**surface** presentation state — the
//! `"mapping"` surface first (the Arrange canvas transform), future views
//! (a node-graph layout, say) getting their own surface key alongside it:
//!
//! ```jsonc
//! {
//!   "format": 1,
//!   "nodes": {
//!     "<node-id>": {
//!       "mapping": {
//!         "transform": { "t": [x, y], "r": deg, "s": scale },
//!         "footprint": { "bbox": [x, y, w, h], "lamps": 150 }
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! # Never a sampling input
//!
//! The transform places a fixture's own doc-space geometry in the shared
//! conceptual space **for human eyes only**. The engine and the device
//! never read this document; no shader, product, or render path may sample
//! through it. A fixture's light is authored by its mapping and its patch —
//! `editor.json` only decides where the picture of it hangs.
//!
//! # Shape rules
//!
//! - Everything is optional per node: a node with no entry (or no
//!   `"mapping"` surface) is simply **unarranged**. Absent transform fields
//!   default to identity (`t: [0,0]`, `r: 0`, `s: 1`), and the writer omits
//!   fields at their defaults.
//! - `footprint` is a derived **cache** for placeholder rendering — the
//!   fixture's doc-space bounding box plus its lamp count, so an unloaded
//!   fixture can render as an honest block. Staleness is acceptable;
//!   refreshing it is the writer's job.
//! - **Unknown surface keys are preserved** on rewrite: a newer build's
//!   surface data survives an older build's save byte-for-byte (both builds
//!   write the same canonical sorted-key form).
//! - `format: 1`, version-and-refuse like every document in this crate:
//!   anything newer refuses whole with the format named, and nothing ever
//!   migrates silently ([`EditorMetaError::UnsupportedFormat`]).
//!
//! # Canonical form
//!
//! [`EditorMetaDoc::to_json_pretty`] writes 2-space indent with **one node
//! entry per line**, node ids and surface keys in sorted order — the
//! text-editor escape hatch applies here as everywhere: a human diff must
//! be readable, one line per fixture.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;

/// Newest `editor.json` format this crate can read.
pub const EDITOR_META_FORMAT: u32 = 1;

/// The parsed `editor.json` document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditorMetaDoc {
    /// Format version gate; see [`EDITOR_META_FORMAT`]. The writer always
    /// stamps [`Self::required_format`].
    pub format: u32,
    /// node id → per-surface editor data. Sorted keys keep writes stable.
    pub nodes: BTreeMap<String, EditorNodeMeta>,
}

/// One node's editor data, keyed by surface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditorNodeMeta {
    /// The `"mapping"` surface: the Arrange canvas placement.
    pub mapping: Option<EditorSurfaceMeta>,
    /// Surfaces this build does not understand, preserved verbatim for the
    /// rewrite (sorted canonical form; see the module doc).
    pub other_surfaces: BTreeMap<String, serde_json::Value>,
}

impl EditorNodeMeta {
    /// A node entry carrying nothing may be dropped by the writer.
    fn is_empty(&self) -> bool {
        self.mapping.is_none() && self.other_surfaces.is_empty()
    }
}

/// One surface's data for one node (today: the `"mapping"` surface).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EditorSurfaceMeta {
    /// Placement in the shared conceptual space. Identity when absent.
    pub transform: EditorTransform,
    /// Cached placeholder footprint; `None` until first written.
    pub footprint: Option<EditorFootprint>,
}

/// Translate + rotate + uniform scale, no shear (ratified).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorTransform {
    /// Translation in doc-space units.
    pub t: [f64; 2],
    /// Rotation in degrees.
    pub r: f64,
    /// Uniform scale.
    pub s: f64,
}

impl EditorTransform {
    /// The identity placement — what an absent transform means.
    pub const IDENTITY: Self = Self {
        t: [0.0, 0.0],
        r: 0.0,
        s: 1.0,
    };

    /// Whether every field is at its default (the writer omits it then).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for EditorTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// The cached placeholder facts: the fixture's own doc-space bounding box
/// (`[x, y, w, h]`) plus its lamp count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorFootprint {
    pub bbox: [f64; 4],
    pub lamps: u32,
}

/// Errors reading an `editor.json` document.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorMetaError {
    /// The document is not valid JSON for this format's shape.
    Parse(String),
    /// The document's `format` is zero or newer than this crate supports.
    UnsupportedFormat { found: u32, supported: u32 },
}

impl core::fmt::Display for EditorMetaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "invalid editor.json document: {reason}"),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "unsupported editor.json format {found} (this build reads up to {supported})"
            ),
        }
    }
}

impl core::error::Error for EditorMetaError {}

impl EditorMetaDoc {
    /// An empty document: every node unarranged.
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: EDITOR_META_FORMAT,
            nodes: BTreeMap::new(),
        }
    }

    /// Parse and format-gate a document. Two-stage like
    /// [`crate::PatchDoc::from_json`]: `format` alone first, so a newer
    /// document reports as newer rather than as malformed.
    pub fn from_json(json: &str) -> Result<Self, EditorMetaError> {
        #[derive(Deserialize)]
        struct FormatPeek {
            format: u32,
        }
        let peek: FormatPeek =
            serde_json::from_str(json).map_err(|e| EditorMetaError::Parse(e.to_string()))?;
        if peek.format == 0 || peek.format > EDITOR_META_FORMAT {
            return Err(EditorMetaError::UnsupportedFormat {
                found: peek.format,
                supported: EDITOR_META_FORMAT,
            });
        }
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| EditorMetaError::Parse(e.to_string()))?;
        let mut nodes = BTreeMap::new();
        match value.get("nodes") {
            None => {}
            Some(serde_json::Value::Object(entries)) => {
                for (node_id, surfaces) in entries {
                    nodes.insert(node_id.clone(), parse_node(node_id, surfaces)?);
                }
            }
            Some(_) => {
                return Err(parse_error("\"nodes\" must be an object keyed by node id"));
            }
        }
        Ok(Self {
            format: peek.format,
            nodes,
        })
    }

    /// The lowest `format` that can represent this document — there is only
    /// one format so far, so this is constant; the method exists to mirror
    /// [`crate::PatchDoc::required_format`] and keep the writer honest when
    /// format 2 arrives.
    #[must_use]
    pub fn required_format(&self) -> u32 {
        EDITOR_META_FORMAT
    }

    /// Stamp [`Self::required_format`] onto the document.
    pub fn normalize_format(&mut self) {
        self.format = self.required_format();
    }

    /// The `"mapping"` surface for a node, if it has one.
    #[must_use]
    pub fn mapping_surface(&self, node: &str) -> Option<&EditorSurfaceMeta> {
        self.nodes.get(node)?.mapping.as_ref()
    }

    /// The `"mapping"` surface for a node, created (empty: identity
    /// transform, no footprint) when absent — the write path for arrange
    /// edits.
    pub fn mapping_surface_mut(&mut self, node: &str) -> &mut EditorSurfaceMeta {
        self.nodes
            .entry(node.to_string())
            .or_default()
            .mapping
            .get_or_insert_with(EditorSurfaceMeta::default)
    }

    /// Serialize compactly (the storage form is [`Self::to_json_pretty`];
    /// this exists for tests and size checks).
    #[must_use]
    pub fn to_json(&self) -> String {
        self.value().to_string()
    }

    /// The canonical pretty form: 2-space indent, ONE node entry per line,
    /// sorted keys throughout.
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        let mut text = String::from("{\n  \"format\": 1");
        let nodes: Vec<(&String, serde_json::Value)> = self
            .nodes
            .iter()
            .filter(|(_, node)| !node.is_empty())
            .map(|(id, node)| (id, node_value(node)))
            .collect();
        if nodes.is_empty() {
            text.push_str("\n}");
            return text;
        }
        text.push_str(",\n  \"nodes\": {\n");
        for (index, (id, value)) in nodes.iter().enumerate() {
            text.push_str("    ");
            text.push_str(&serde_json::Value::String((*id).clone()).to_string());
            text.push_str(": ");
            text.push_str(&value.to_string());
            text.push_str(if index + 1 < nodes.len() { ",\n" } else { "\n" });
        }
        text.push_str("  }\n}");
        text
    }

    /// The whole document as a JSON value (compact writer's source).
    fn value(&self) -> serde_json::Value {
        let mut root = serde_json::Map::new();
        root.insert("format".into(), self.required_format().into());
        let nodes: serde_json::Map<String, serde_json::Value> = self
            .nodes
            .iter()
            .filter(|(_, node)| !node.is_empty())
            .map(|(id, node)| (id.clone(), node_value(node)))
            .collect();
        if !nodes.is_empty() {
            root.insert("nodes".into(), serde_json::Value::Object(nodes));
        }
        serde_json::Value::Object(root)
    }
}

/// One node's surfaces object → [`EditorNodeMeta`].
fn parse_node(node_id: &str, value: &serde_json::Value) -> Result<EditorNodeMeta, EditorMetaError> {
    let serde_json::Value::Object(surfaces) = value else {
        return Err(parse_error(&alloc::format!(
            "node {node_id:?} must be an object keyed by surface"
        )));
    };
    let mut node = EditorNodeMeta::default();
    for (surface, data) in surfaces {
        if surface == "mapping" {
            node.mapping = Some(parse_surface(node_id, data)?);
        } else {
            node.other_surfaces.insert(surface.clone(), data.clone());
        }
    }
    Ok(node)
}

/// One `"mapping"` surface object.
fn parse_surface(
    node_id: &str,
    value: &serde_json::Value,
) -> Result<EditorSurfaceMeta, EditorMetaError> {
    let surface_error = |reason: &str| {
        parse_error(&alloc::format!(
            "node {node_id:?} mapping surface: {reason}"
        ))
    };
    let serde_json::Value::Object(fields) = value else {
        return Err(surface_error("must be an object"));
    };
    let transform = match fields.get("transform") {
        None => EditorTransform::IDENTITY,
        Some(serde_json::Value::Object(t)) => EditorTransform {
            t: match t.get("t") {
                None => [0.0, 0.0],
                Some(value) => {
                    parse_floats::<2>(value).ok_or_else(|| surface_error("\"t\" must be [x, y]"))?
                }
            },
            r: match t.get("r") {
                None => 0.0,
                Some(value) => value
                    .as_f64()
                    .ok_or_else(|| surface_error("\"r\" must be a number (degrees)"))?,
            },
            s: match t.get("s") {
                None => 1.0,
                Some(value) => value
                    .as_f64()
                    .ok_or_else(|| surface_error("\"s\" must be a number (uniform scale)"))?,
            },
        },
        Some(_) => return Err(surface_error("\"transform\" must be an object")),
    };
    let footprint = match fields.get("footprint") {
        None => None,
        Some(serde_json::Value::Object(fp)) => Some(EditorFootprint {
            bbox: fp
                .get("bbox")
                .and_then(parse_floats::<4>)
                .ok_or_else(|| surface_error("\"footprint.bbox\" must be [x, y, w, h]"))?,
            lamps: fp
                .get("lamps")
                .and_then(serde_json::Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| surface_error("\"footprint.lamps\" must be a lamp count"))?,
        }),
        Some(_) => return Err(surface_error("\"footprint\" must be an object")),
    };
    Ok(EditorSurfaceMeta {
        transform,
        footprint,
    })
}

/// A fixed-length array of JSON numbers.
fn parse_floats<const N: usize>(value: &serde_json::Value) -> Option<[f64; N]> {
    let items = value.as_array()?;
    if items.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = item.as_f64()?;
    }
    Some(out)
}

/// One node's surfaces as a JSON value: `"mapping"` typed, unknown surfaces
/// verbatim, sorted keys (serde_json maps sort by construction).
///
/// Floats are QUANTIZED to canonical precision before the default checks,
/// so a value that quantizes to a default is omitted on the first write,
/// not the second — write → parse → write must be the identity.
fn node_value(node: &EditorNodeMeta) -> serde_json::Value {
    let mut surfaces = serde_json::Map::new();
    if let Some(mapping) = &node.mapping {
        let transform = EditorTransform {
            t: [
                quantize(mapping.transform.t[0]),
                quantize(mapping.transform.t[1]),
            ],
            r: quantize(mapping.transform.r),
            s: quantize(mapping.transform.s),
        };
        let mut fields = serde_json::Map::new();
        if !transform.is_identity() {
            let mut t = serde_json::Map::new();
            if transform.t != [0.0, 0.0] {
                t.insert("t".into(), float_array(&transform.t));
            }
            if transform.r != 0.0 {
                t.insert("r".into(), float_value(transform.r));
            }
            if transform.s != 1.0 {
                t.insert("s".into(), float_value(transform.s));
            }
            fields.insert("transform".into(), serde_json::Value::Object(t));
        }
        if let Some(footprint) = &mapping.footprint {
            let mut fp = serde_json::Map::new();
            fp.insert("bbox".into(), float_array(&footprint.bbox));
            fp.insert("lamps".into(), footprint.lamps.into());
            fields.insert("footprint".into(), serde_json::Value::Object(fp));
        }
        surfaces.insert("mapping".into(), serde_json::Value::Object(fields));
    }
    for (surface, data) in &node.other_surfaces {
        surfaces.insert(surface.clone(), data.clone());
    }
    serde_json::Value::Object(surfaces)
}

fn float_array(values: &[f64]) -> serde_json::Value {
    serde_json::Value::Array(values.iter().map(|v| float_value(quantize(*v))).collect())
}

/// Canonical float precision: 4 decimals of a doc-space unit —
/// presentation data needs no more, hand-edited diffs stay readable, and
/// the short mantissa keeps serde_json's fast-path parse EXACT (the
/// `float_roundtrip` parser feature is off workspace-wide, and 17-digit
/// mantissas can come back one ulp different — measured on the dome's f32
/// bounds).
fn quantize(value: f64) -> f64 {
    libm::round(value * 10_000.0) / 10_000.0
}

/// Whole-valued floats write as integers (`40` not `40.0`) — hand-authored
/// documents and the writer agree on the shorter spelling. Callers pass
/// [`quantize`]d values.
fn float_value(value: f64) -> serde_json::Value {
    if value == libm::trunc(value) && libm::fabs(value) < 1e15 {
        serde_json::Value::from(value as i64)
    } else {
        serde_json::Value::from(value)
    }
}

fn parse_error(reason: &str) -> EditorMetaError {
    EditorMetaError::Parse(reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arranged(t: [f64; 2], r: f64, s: f64) -> EditorSurfaceMeta {
        EditorSurfaceMeta {
            transform: EditorTransform { t, r, s },
            footprint: None,
        }
    }

    #[test]
    fn an_empty_document_round_trips() {
        let doc = EditorMetaDoc::new();
        assert_eq!(doc.to_json_pretty(), "{\n  \"format\": 1\n}");
        let parsed = EditorMetaDoc::from_json(&doc.to_json_pretty()).unwrap();
        assert_eq!(parsed, doc);
    }

    #[test]
    fn a_populated_document_round_trips_byte_stably() {
        let mut doc = EditorMetaDoc::new();
        *doc.mapping_surface_mut("node-b") = arranged([40.0, -12.5], 90.0, 1.0);
        *doc.mapping_surface_mut("node-a") = EditorSurfaceMeta {
            transform: EditorTransform::IDENTITY,
            footprint: Some(EditorFootprint {
                bbox: [0.0, 0.0, 60.0, 42.5],
                lamps: 150,
            }),
        };
        let pretty = doc.to_json_pretty();
        let parsed = EditorMetaDoc::from_json(&pretty).unwrap();
        assert_eq!(parsed, doc);
        assert_eq!(parsed.to_json_pretty(), pretty, "second round trip");
        // One node entry per line: `{`, format, `"nodes": {`, 2 nodes, `}`, `}`.
        assert_eq!(pretty.lines().count(), 7, "{pretty}");
        // Sorted node order, defaults omitted, whole floats written short.
        assert!(pretty.contains(r#""t":[40,-12.5]"#), "{pretty}");
        assert!(pretty.contains(r#""r":90"#), "{pretty}");
        assert!(!pretty.contains(r#""s""#), "{pretty}");
        let a = pretty.find("node-a").unwrap();
        let b = pretty.find("node-b").unwrap();
        assert!(a < b, "{pretty}");
    }

    #[test]
    fn absent_transform_fields_default_to_identity() {
        let doc = EditorMetaDoc::from_json(
            r#"{"format":1,"nodes":{"n":{"mapping":{"transform":{"r":45}}}}}"#,
        )
        .unwrap();
        let surface = doc.mapping_surface("n").unwrap();
        assert_eq!(surface.transform.t, [0.0, 0.0]);
        assert_eq!(surface.transform.r, 45.0);
        assert_eq!(surface.transform.s, 1.0);

        let empty =
            EditorMetaDoc::from_json(r#"{"format":1,"nodes":{"n":{"mapping":{}}}}"#).unwrap();
        assert!(
            empty.mapping_surface("n").unwrap().transform.is_identity(),
            "an empty mapping surface is arranged at identity"
        );
        assert_eq!(empty.mapping_surface("missing"), None);
    }

    /// A newer build's surface data survives an older build's save — parse
    /// a document with a surface this build does not know, rewrite, and the
    /// unknown surface is still there byte-for-byte.
    #[test]
    fn unknown_surfaces_survive_the_rewrite() {
        let text = r#"{
  "format": 1,
  "nodes": {
    "n": {"graph":{"pins":[1,2,3],"pos":{"x":4,"y":5}},"mapping":{"transform":{"r":90}}}
  }
}"#;
        let mut doc = EditorMetaDoc::from_json(text).unwrap();
        // The older build edits what it understands...
        doc.mapping_surface_mut("n").transform.r = 45.0;
        let written = doc.to_json_pretty();
        // ...and the unknown surface rides through untouched.
        assert!(
            written.contains(r#""graph":{"pins":[1,2,3],"pos":{"x":4,"y":5}}"#),
            "{written}"
        );
        let reparsed = EditorMetaDoc::from_json(&written).unwrap();
        assert_eq!(reparsed.to_json_pretty(), written, "byte-stable rewrite");
        assert_eq!(
            reparsed.nodes["n"].other_surfaces["graph"],
            serde_json::json!({"pins":[1,2,3],"pos":{"x":4,"y":5}})
        );
    }

    #[test]
    fn newer_and_zero_formats_are_refused_whole() {
        for (text, found) in [
            (r#"{"format":2,"nodes":{}}"#, 2),
            (r#"{"format":0}"#, 0),
            (r#"{"format":9,"anything":["goes"]}"#, 9),
        ] {
            assert_eq!(
                EditorMetaDoc::from_json(text),
                Err(EditorMetaError::UnsupportedFormat {
                    found,
                    supported: EDITOR_META_FORMAT
                })
            );
        }
    }

    #[test]
    fn broken_documents_report_as_broken() {
        for text in [
            "{",
            r#"{"nodes":{}}"#,
            r#"{"format":1,"nodes":[]}"#,
            r#"{"format":1,"nodes":{"n":[]}}"#,
            r#"{"format":1,"nodes":{"n":{"mapping":{"transform":{"t":[1]}}}}}"#,
            r#"{"format":1,"nodes":{"n":{"mapping":{"footprint":{"bbox":[0,0,1,1]}}}}}"#,
        ] {
            assert!(
                matches!(
                    EditorMetaDoc::from_json(text),
                    Err(EditorMetaError::Parse(_))
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn create_on_write_and_empty_entries_are_dropped() {
        let mut doc = EditorMetaDoc::new();
        doc.nodes.insert("hollow".into(), EditorNodeMeta::default());
        doc.mapping_surface_mut("n").transform.t = [1.0, 2.0];
        let written = doc.to_json_pretty();
        assert!(!written.contains("hollow"), "{written}");
        assert!(written.contains(r#""t":[1,2]"#), "{written}");
    }

    /// High-precision floats (an f32-derived bbox) quantize to canonical
    /// 4-decimal form on the FIRST write and stay byte-stable after — the
    /// fast-path JSON parse must never perturb the canonical bytes.
    #[test]
    fn floats_quantize_once_and_rewrite_byte_stably() {
        let mut doc = EditorMetaDoc::new();
        *doc.mapping_surface_mut("n") = EditorSurfaceMeta {
            transform: EditorTransform {
                t: [13.683_242_797_851_562, 0.000_04],
                r: 0.0,
                s: 1.000_04,
            },
            footprint: Some(EditorFootprint {
                bbox: [
                    13.683_242_797_851_562,
                    10.0,
                    95.105_651_855_468_76,
                    92.801_986_694_335_94,
                ],
                lamps: 150,
            }),
        };
        let written = doc.to_json_pretty();
        assert!(written.contains(r#""t":[13.6832,0]"#), "{written}");
        assert!(
            !written.contains(r#""s""#),
            "a scale that quantizes to 1 is omitted on the FIRST write: {written}"
        );
        assert!(
            written.contains(r#""bbox":[13.6832,10,95.1057,92.802]"#),
            "{written}"
        );
        let reparsed = EditorMetaDoc::from_json(&written).unwrap();
        assert_eq!(reparsed.to_json_pretty(), written, "byte-stable rewrite");
    }

    #[test]
    fn footprints_round_trip() {
        let mut doc = EditorMetaDoc::new();
        doc.mapping_surface_mut("n").footprint = Some(EditorFootprint {
            bbox: [-30.0, -30.0, 60.0, 60.0],
            lamps: 177,
        });
        let parsed = EditorMetaDoc::from_json(&doc.to_json_pretty()).unwrap();
        assert_eq!(
            parsed.mapping_surface("n").unwrap().footprint,
            Some(EditorFootprint {
                bbox: [-30.0, -30.0, 60.0, 60.0],
                lamps: 177
            })
        );
    }
}
